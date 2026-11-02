//! cb-extract: HTML -> AI-readable markdown, with main-content scoring and token budgeting.

use cb_dom::Document;
use scraper::{ElementRef, Selector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub alt: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Markdown {
    pub text: String,
    pub tokens_estimate: usize,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractOpts {
    pub scope: Option<String>,
    pub max_tokens: Option<usize>,
    pub inline_link_limit: usize,
}

impl Default for ExtractOpts {
    fn default() -> Self {
        ExtractOpts {
            scope: None,
            max_tokens: None,
            inline_link_limit: 30,
        }
    }
}

fn sel(css: &str) -> Option<Selector> {
    Selector::parse(css).ok()
}

const CONTAINER_TAGS: &[&str] = &[
    "body", "main", "article", "section", "div", "figure", "details", "dl", "td", "th",
];
const LIST_TAGS: &[&str] = &["ul", "ol"];

struct Ctx {
    inline_links: bool,
    links: Vec<Link>,
    images: Vec<Image>,
}

/// 折叠空白，但保留首尾各一个空格（行内元素之间需要靠它分词）。
fn collapse_keep_edges(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            prev_space = false;
            out.push(ch);
        }
    }
    out
}

fn collapse(text: &str) -> String {
    collapse_keep_edges(text).trim().to_string()
}

fn inline_text(ctx: &mut Ctx, el: ElementRef<'_>) -> String {
    let mut out = String::new();
    for child in el.children() {
        inline_node(ctx, child, &mut out);
    }
    out
}

fn inline_node(ctx: &mut Ctx, node: ego_tree::NodeRef<'_, scraper::Node>, out: &mut String) {
    match node.value() {
        scraper::Node::Text(t) => out.push_str(&collapse_keep_edges(&t.text)),
        scraper::Node::Element(_) => {
            let el = ElementRef::wrap(node).expect("element");
            if cb_dom::is_noise(&el) {
                return;
            }
            match el.value().name() {
                "strong" | "b" => {
                    let inner = inline_text(ctx, el);
                    let t = inner.trim();
                    if !t.is_empty() {
                        out.push_str(&format!("**{t}**"));
                    }
                }
                "em" | "i" => {
                    let inner = inline_text(ctx, el);
                    let t = inner.trim();
                    if !t.is_empty() {
                        out.push_str(&format!("*{t}*"));
                    }
                }
                "code" => {
                    let t: String = el.text().collect();
                    if !t.trim().is_empty() {
                        out.push_str(&format!("`{}`", t.trim()));
                    }
                }
                "a" => {
                    let href = el
                        .value()
                        .attr("href")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let inner = inline_text(ctx, el);
                    let text = inner.trim();
                    if text.is_empty() {
                        return;
                    }
                    if href.is_empty() {
                        out.push_str(text);
                        return;
                    }
                    ctx.links.push(Link {
                        text: text.to_string(),
                        url: href.clone(),
                    });
                    if ctx.inline_links {
                        out.push_str(&format!("[{text}]({href})"));
                    } else {
                        out.push_str(text);
                    }
                }
                "img" => {
                    let src = el
                        .value()
                        .attr("src")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if src.is_empty() || src.starts_with("data:") {
                        return;
                    }
                    let alt = el
                        .value()
                        .attr("alt")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    ctx.images.push(Image {
                        alt: alt.clone(),
                        url: src.clone(),
                    });
                    out.push_str(&format!("![{alt}]({src})"));
                }
                "br" => out.push(' '),
                _ => {
                    let inner = inline_text(ctx, el);
                    out.push_str(&inner);
                }
            }
        }
        _ => {}
    }
}

fn code_fence_language(el: ElementRef<'_>) -> String {
    let code = sel("code").and_then(|s| el.select(&s).next()).unwrap_or(el);
    let class = code.value().attr("class").unwrap_or_default();
    for token in class.split_whitespace() {
        if let Some(lang) = token.strip_prefix("language-") {
            return lang.to_string();
        }
        if let Some(lang) = token.strip_prefix("highlight-source-") {
            return lang.to_string();
        }
    }
    String::new()
}

fn push_inline_block(ctx: &mut Ctx, el: ElementRef<'_>, blocks: &mut Vec<String>) {
    let text = inline_text(ctx, el);
    let t = text.trim();
    if !t.is_empty() {
        blocks.push(t.to_string());
    }
}

fn has_block_children(el: ElementRef<'_>) -> bool {
    el.children().any(|c| {
        c.value()
            .as_element()
            .map(|e| !CONTAINER_TAGS.contains(&e.name()) || e.name() == "body")
            .unwrap_or(false)
    })
}

fn render_children(ctx: &mut Ctx, el: ElementRef<'_>, indent: usize, blocks: &mut Vec<String>) {
    for child in el.children() {
        if let Some(child_el) = ElementRef::wrap(child) {
            render_element(ctx, child_el, indent, blocks);
        }
    }
}

fn render_element(ctx: &mut Ctx, el: ElementRef<'_>, indent: usize, blocks: &mut Vec<String>) {
    if cb_dom::is_noise(&el) {
        return;
    }
    let name = el.value().name();
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = name[1..].parse::<usize>().unwrap_or(1);
            let text = inline_text(ctx, el);
            let t = text.trim();
            if !t.is_empty() {
                blocks.push(format!("{} {t}", "#".repeat(level)));
            }
        }
        "p" | "div" | "section" | "article" | "main" | "body" | "figure" | "details" | "dl" => {
            if name == "p" || !has_block_children(el) {
                push_inline_block(ctx, el, blocks);
            } else {
                render_children(ctx, el, indent, blocks);
            }
        }
        "pre" => {
            let lang = code_fence_language(el);
            let code = sel("code").and_then(|s| el.select(&s).next()).unwrap_or(el);
            let body = code
                .text()
                .collect::<String>()
                .trim_end_matches('\n')
                .to_string();
            blocks.push(format!("```{lang}\n{body}\n```"));
        }
        "blockquote" => {
            let mut inner = Vec::new();
            render_children(ctx, el, indent, &mut inner);
            if !inner.is_empty() {
                let quoted: Vec<String> = inner
                    .join("\n\n")
                    .lines()
                    .map(|l| {
                        if l.trim().is_empty() {
                            ">".to_string()
                        } else {
                            format!("> {l}")
                        }
                    })
                    .collect();
                blocks.push(quoted.join("\n"));
            }
        }
        "table" => {
            if let Some(table) = render_table(el) {
                blocks.push(table);
            }
        }
        "ul" | "ol" => {
            let mut lines = Vec::new();
            render_list(ctx, el, indent, &mut lines);
            if !lines.is_empty() {
                blocks.push(lines.join("\n"));
            }
        }
        "hr" => blocks.push("---".to_string()),
        _ => {
            if LIST_TAGS.contains(&name) {
                return;
            }
            push_inline_block(ctx, el, blocks);
        }
    }
}

fn render_list(ctx: &mut Ctx, list: ElementRef<'_>, indent: usize, out: &mut Vec<String>) {
    let ordered = list.value().name() == "ol";
    let pad = "  ".repeat(indent);
    let mut index = 0usize;
    let items: Vec<ElementRef<'_>> = match sel("li") {
        Some(s) => list.select(&s).collect(),
        None => return,
    };
    for li in items {
        let direct = li.parent().and_then(ElementRef::wrap).map(|p| p.id()) == Some(list.id());
        if !direct {
            continue;
        }
        index += 1;
        let marker = if ordered {
            format!("{index}. ")
        } else {
            "- ".to_string()
        };
        let mut nested = Vec::new();
        let mut text = String::new();
        for child in li.children() {
            if let Some(child_el) = ElementRef::wrap(child) {
                if LIST_TAGS.contains(&child_el.value().name()) {
                    render_list(ctx, child_el, indent + 1, &mut nested);
                    continue;
                }
            }
            inline_node(ctx, child, &mut text);
        }
        let t = text.trim();
        if t.is_empty() && nested.is_empty() {
            continue;
        }
        out.push(format!("{pad}{marker}{t}"));
        out.extend(nested);
    }
}

fn render_table(table: ElementRef<'_>) -> Option<String> {
    let rows_sel = sel("tr")?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in table.select(&rows_sel) {
        let mut cells = Vec::new();
        for cell in tr.children().filter_map(ElementRef::wrap) {
            let name = cell.value().name();
            if name != "td" && name != "th" {
                continue;
            }
            let text: String = cell.text().collect::<Vec<_>>().join(" ");
            cells.push(collapse(&text).replace('|', "\\|"));
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let mut cells = row.clone();
        while cells.len() < width {
            cells.push(String::new());
        }
        out.push_str(&format!("| {} |", cells.join(" | ")));
        out.push('\n');
        if i == 0 {
            out.push_str(&format!("| {} |", vec!["---"; width].join(" | ")));
            out.push('\n');
        }
    }
    Some(out.trim_end().to_string())
}

pub fn extract_markdown(html: &str, opts: &ExtractOpts) -> anyhow::Result<Markdown> {
    let doc = Document::parse(html);
    let root = match &opts.scope {
        Some(css) => doc
            .select(css)
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("scope selector matched nothing: {css}"))?,
        None => doc
            .main_content()
            .or_else(|| doc.body())
            .ok_or_else(|| anyhow::anyhow!("no content container found"))?,
    };

    let all_links: Vec<Link> = match sel("a") {
        Some(s) => root
            .select(&s)
            .filter_map(|a| {
                let href = a.value().attr("href")?.trim().to_string();
                if href.is_empty() {
                    return None;
                }
                let text = collapse(&a.text().collect::<Vec<_>>().join(" "));
                if text.is_empty() {
                    return None;
                }
                Some(Link { text, url: href })
            })
            .collect(),
        None => Vec::new(),
    };
    let all_images: Vec<Image> = match sel("img") {
        Some(s) => root
            .select(&s)
            .filter_map(|i| {
                let src = i.value().attr("src")?.trim().to_string();
                if src.is_empty() || src.starts_with("data:") {
                    return None;
                }
                Some(Image {
                    alt: i.value().attr("alt").unwrap_or_default().trim().to_string(),
                    url: src,
                })
            })
            .collect(),
        None => Vec::new(),
    };

    let inline_links = all_links.len() <= opts.inline_link_limit;
    let mut ctx = Ctx {
        inline_links,
        links: Vec::new(),
        images: Vec::new(),
    };
    let mut blocks = Vec::new();
    render_element(&mut ctx, root, 0, &mut blocks);

    if !ctx.links.is_empty() && !inline_links {
        let mut list = String::from("## Links");
        for l in &ctx.links {
            list.push_str(&format!("\n\n- [{}]({})", l.text, l.url));
        }
        blocks.push(list);
    }

    let links = if ctx.links.is_empty() {
        all_links
    } else {
        ctx.links
    };
    let images = if ctx.images.is_empty() {
        all_images
    } else {
        ctx.images
    };

    let mut text = blocks.join("\n\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }

    let mut truncated = false;
    if let Some(budget) = opts.max_tokens {
        if estimate_tokens(&text) > budget {
            truncated = true;
            let mut kept: Vec<String> = Vec::new();
            let mut used = 0usize;
            for block in &blocks {
                let cost = estimate_tokens(block) + 1;
                if used + cost > budget && !kept.is_empty() {
                    break;
                }
                used += cost;
                kept.push(block.clone());
            }
            text = kept.join("\n\n");
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let max_chars = budget.saturating_mul(4);
            if text.chars().count() > max_chars {
                text = text.chars().take(max_chars).collect();
                text.push('\n');
            }
        }
    }

    let tokens_estimate = estimate_tokens(&text);
    Ok(Markdown {
        text,
        tokens_estimate,
        links,
        images,
        truncated,
    })
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() + 3) / 4
}

/// 便捷入口：用默认选项提取 markdown。
pub fn extract(html: &str) -> anyhow::Result<Markdown> {
    extract_markdown(html, &ExtractOpts::default())
}
