//! cb-extract: HTML -> AI-readable markdown, with main-content scoring and token budgeting.

use cb_dom::Document;
use scraper::{ElementRef, Selector};
use serde::{Deserialize, Serialize};

pub mod blocks;
pub mod doc;

pub use blocks::{Block, LinkRef, NoticeLevel, RenderDoc, Span, SpanStyle};
pub use doc::{notice_doc, to_document};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub alt: String,
    pub url: String,
    /// 是不是从 srcset / data-src 这类懒加载属性里取到的。
    #[serde(default)]
    pub lazy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Markdown {
    pub text: String,
    pub tokens_estimate: usize,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    pub truncated: bool,
    /// 选中容器的正文字数占整页正文的比例（0.0-1.0，链接密度会扣分）。
    /// 调用方用它判断「抓没抓错容器」；列表页/整页包裹会明显偏低。
    #[serde(default)]
    pub container_score: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ExtractOpts {
    pub scope: Option<String>,
    pub max_tokens: Option<usize>,
    pub inline_link_limit: usize,
    /// 是否把图片内联进 markdown（默认开；--no-images 关）。
    pub images: bool,
}

impl Default for ExtractOpts {
    fn default() -> Self {
        ExtractOpts {
            scope: None,
            max_tokens: None,
            inline_link_limit: 30,
            images: true,
        }
    }
}

/// 占位图判定：尺寸 ≤2×2、URL 含占位词、或短 data URI。
pub(crate) fn placeholder_url(url: &str, w: Option<u32>, h: Option<u32>) -> bool {
    if let (Some(w), Some(h)) = (w, h) {
        if w <= 2 && h <= 2 {
            return true;
        }
    }
    let l = url.to_ascii_lowercase();
    let words = [
        "placeholder",
        "blank.gif",
        "blank.png",
        "loading.gif",
        "spacer",
        "1x1",
        "transparent",
        "pixel.gif",
        "grey.gif",
    ];
    if words.iter().any(|m| l.contains(m)) {
        return true;
    }
    url.starts_with("data:") && url.len() < 200
}

/// srcset 里选最大的一张：按描述符（w 或 x）排序，没有描述符就取最后一项。
fn pick_srcset(srcset: &str) -> Option<String> {
    let mut best: Option<(f32, String)> = None;
    for part in srcset.split(',') {
        let mut it = part.split_whitespace();
        let Some(url) = it.next() else { continue };
        let weight = it
            .next()
            .and_then(|d| d.trim_end_matches(['w', 'x']).parse::<f32>().ok())
            .unwrap_or(0.0);
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        match &best {
            Some((bw, _)) if *bw >= weight => {}
            _ => best = Some((weight, url.to_string())),
        }
    }
    best.map(|(_, u)| u)
}

/// 图片真实地址：srcset > data-src > data-original > data-lazy-src > data-actualsrc > src。
pub(crate) fn best_image_src(el: ElementRef<'_>) -> Option<(String, bool)> {
    let w = el
        .value()
        .attr("width")
        .and_then(|v| v.trim().parse::<u32>().ok());
    let h = el
        .value()
        .attr("height")
        .and_then(|v| v.trim().parse::<u32>().ok());
    if let Some(srcset) = el.value().attr("srcset") {
        if let Some(url) = pick_srcset(srcset) {
            if !placeholder_url(&url, w, h) {
                return Some((url, true));
            }
        }
    }
    for attr in [
        "data-src",
        "data-original",
        "data-lazy-src",
        "data-actualsrc",
        "data-original-src",
    ] {
        if let Some(v) = el.value().attr(attr) {
            let v = v.trim();
            if !v.is_empty() && !placeholder_url(v, w, h) {
                return Some((v.to_string(), true));
            }
        }
    }
    let src = el.value().attr("src")?.trim();
    if src.is_empty() || placeholder_url(src, w, h) {
        return None;
    }
    Some((src.to_string(), false))
}

fn sel(css: &str) -> Option<Selector> {
    Selector::parse(css).ok()
}

/// 记录并（可选地）把一张图写成独立一行的 markdown。
fn push_image(ctx: &mut Ctx, el: ElementRef<'_>, out: &mut String) {
    let Some((src, lazy)) = best_image_src(el) else {
        return;
    };
    let alt = el
        .value()
        .attr("alt")
        .unwrap_or_default()
        .trim()
        .to_string();
    ctx.images.push(Image {
        alt: alt.clone(),
        url: src.clone(),
        lazy,
    });
    if !ctx.emit_images {
        return;
    }
    // 图片独立成行：夹在段落文字里会导致 markdown 渲染与文不对图。
    let label = image_label(&alt, &src);
    out.push_str(&format!("\n\n![{label}]({src})\n\n"));
}

/// 图片在 markdown 里的标签：alt 为空时退回文件名。
///
/// 空 alt 的图片会被部分渲染器当装饰图跳过，也会让 AI 不知道这是什么。
pub(crate) fn image_label(alt: &str, url: &str) -> String {
    let alt = alt.trim();
    if !alt.is_empty() {
        return alt.to_string();
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("").trim();
    if name.is_empty() {
        "image".to_string()
    } else {
        name.to_string()
    }
}

const LIST_TAGS: &[&str] = &["ul", "ol"];

struct Ctx {
    inline_links: bool,
    links: Vec<Link>,
    images: Vec<Image>,
    /// 是否把图片内联进 markdown。
    emit_images: bool,
    /// 正文容器是否语义化（article/main）：决定页面级页眉页脚要不要保留。
    semantic_root: bool,
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
                        // 链接包着图片时标签是空文本，但它里面的图不能丢。
                        out.push_str(&inner);
                        return;
                    }
                    if href.is_empty() {
                        out.push_str(text);
                        return;
                    }
                    // 相邻链接之间补空格，避免 markdown 里粘成一坨。
                    if let Some(last) = out.chars().last() {
                        if !last.is_whitespace() && !"|，。、；：([".contains(last) {
                            out.push(' ');
                        }
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
                "img" => push_image(ctx, el, out),
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
    if !t.is_empty() && !looks_like_json_dump(t) {
        blocks.push(t.to_string());
    }
}

/// 块级标签真集合：子树里有这些就递归，否则整块当行内段落。
const BLOCK_TAGS: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "div",
    "section",
    "article",
    "main",
    "header",
    "footer",
    "nav",
    "aside",
    "ul",
    "ol",
    "li",
    "table",
    "thead",
    "tbody",
    "tr",
    "td",
    "th",
    "pre",
    "blockquote",
    "hr",
    "dl",
    "dt",
    "dd",
    "figure",
    "figcaption",
    "form",
    "fieldset",
    "details",
    "summary",
    "body",
];

fn has_block_children(el: ElementRef<'_>) -> bool {
    el.children()
        .filter_map(ElementRef::wrap)
        .any(|c| BLOCK_TAGS.contains(&c.value().name()))
}

/// 形态像 JSON 数据转储的文本不是正文，丢掉。
fn looks_like_json_dump(text: &str) -> bool {
    let t = text.trim();
    if t.chars().count() < 120 {
        return false;
    }
    let pairs = t.matches("\":").count() + t.matches("\": ").count();
    let brackets = t.matches('{').count() + t.matches('[').count();
    pairs >= 3 && brackets >= 1
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
    // 页面级页眉页脚默认丢掉；但文章自己的 header（标题/byline）要留。
    if cb_dom::is_page_chrome(&el) && !ctx.semantic_root {
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
        // 结构性容器永远递归，绝不把整页压成一行。
        "body" | "main" | "article" => render_children(ctx, el, indent, blocks),
        "p" | "div" | "section" | "figure" | "details" | "dl" => {
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
        // 直接挂在容器下的图（不在 p/figure 里）也要出现在正文里。
        "img" => {
            let mut image = String::new();
            push_image(ctx, el, &mut image);
            let t = image.trim();
            if !t.is_empty() {
                blocks.push(t.to_string());
            }
        }
        // 图注单独一行斜体，跟在前面的图后面。
        "figcaption" => {
            let t = inline_text(ctx, el);
            let t = t.trim();
            if !t.is_empty() {
                blocks.push(format!("*{t}*"));
            }
        }
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
    let (root, semantic_root) = match &opts.scope {
        Some(css) => (
            doc.select(css)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("scope selector matched nothing: {css}"))?,
            true,
        ),
        None => {
            let el = doc
                .main_content()
                .or_else(|| doc.body())
                .ok_or_else(|| anyhow::anyhow!("no content container found"))?;
            let semantic = matches!(el.value().name(), "article" | "main")
                || el
                    .value()
                    .attr("role")
                    .map(|role| role.eq_ignore_ascii_case("main"))
                    .unwrap_or(false);
            (el, semantic)
        }
    };

    let container_score = container_score_of(&doc, root);
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
    let inline_links = all_links.len() <= opts.inline_link_limit;
    let mut ctx = Ctx {
        inline_links,
        links: Vec::new(),
        images: Vec::new(),
        emit_images: opts.images,
        semantic_root,
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
    // 只报「markdown 里真实出现过的图」，保证 md 与 images[] 是同一份事实。
    let images = ctx.images;

    let mut text = squeeze_blank_lines(&blocks.join("\n\n"));
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }

    let mut truncated = false;
    if let Some(budget) = opts.max_tokens {
        if estimate_tokens(&text) > budget {
            truncated = true;
            // 图不能因为预算被删：宁可少留一段文字。
            let has_image = |block: &str| block.contains("![");
            let mut kept: Vec<String> = Vec::new();
            let mut used = 0usize;
            let mut kept_image = false;
            for block in &blocks {
                let cost = estimate_tokens(block) + 1;
                if has_image(block) {
                    used += cost;
                    kept_image = true;
                    kept.push(block.clone());
                    continue;
                }
                if used + cost > budget && !kept.is_empty() {
                    continue;
                }
                used += cost;
                kept.push(block.clone());
            }
            text = squeeze_blank_lines(&kept.join("\n\n"));
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            // 字符上限是 token 估算的保险丝；带图时不砍，图片 URL 会把它提前顶爆。
            let max_chars = budget.saturating_mul(4);
            if !kept_image && text.chars().count() > max_chars {
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
        container_score,
    })
}

/// 容器得分：选中容器的文本占 body 的比例，再按容器内链接密度扣分。
///
/// 只看绝对字符数无法区分「抓对了」和「抓多了」；列表页与整页包裹层
/// 文本不少但链接密度高，必须压下去。
fn container_score_of(doc: &Document, root: ElementRef<'_>) -> Option<f64> {
    let body = doc.body()?;
    let body_len = doc.text_of(body).chars().count();
    if body_len == 0 {
        return None;
    }
    let root_len = doc.text_of(root).chars().count();
    let share = (root_len as f64 / body_len as f64).clamp(0.0, 1.0);
    let density = doc.link_density(root);
    Some((share * (1.0 - density)).clamp(0.0, 1.0))
}

/// 连续空行压成一个空行；图片独立成行会制造多余换行。
fn squeeze_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newlines = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(ch);
    }
    out
}

/// token 估算：CJK 一字≈一 token，其余按 4 字符≈1 token。
/// 纯 chars/4 在中文页面上会低估 3-4 倍，直接烧调用方的预算。
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x11FF      // 韩文字母
        | 0x2E80..=0x303F    // CJK 部首、标点
        | 0x3040..=0x30FF    // 假名
        | 0x3400..=0x4DBF    // 扩展 A
        | 0x4E00..=0x9FFF    // 基本汉字
        | 0xAC00..=0xD7AF    // 韩文音节
        | 0xF900..=0xFAFF    // 兼容汉字
        | 0x20000..=0x2FA1F) // 扩展 B-F
}

/// 便捷入口：用默认选项提取 markdown。
pub fn extract(html: &str) -> anyhow::Result<Markdown> {
    extract_markdown(html, &ExtractOpts::default())
}
/// markdown → 纯文本（`--to text`）：去掉标记，但保留结构与可读性。
pub fn to_plain_text(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_fence = false;
    for line in markdown.lines() {
        let trimmed = line.trim_end();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if trimmed.trim().is_empty() {
            out.push('\n');
            continue;
        }
        let mut text = trimmed.to_string();
        if let Some(rest) = text.strip_prefix('>') {
            text = rest.trim_start().to_string();
        }
        while text.starts_with('#') {
            text.remove(0);
        }
        let text = text.trim_start().to_string();
        let text = if text.starts_with('|') {
            let cells: Vec<String> = text
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.chars().all(|ch| ch == '-' || ch == ':'))
                .collect();
            cells.join("\t")
        } else {
            text
        };
        out.push_str(&inline_to_plain(&text));
        out.push('\n');
    }
    let mut squeezed = String::with_capacity(out.len());
    let mut blank_run = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        squeezed.push_str(line);
        squeezed.push('\n');
    }
    squeezed.trim_start_matches('\n').to_string()
}

fn inline_to_plain(line: &str) -> String {
    // [text](url) -> text (url)；![alt](url) -> [image: alt] url；去掉 ** * ` 标记。
    let mut out = String::with_capacity(line.len());
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == '!' && i + 1 < bytes.len() && bytes[i + 1] == '[' {
            if let Some((label, url, next)) = parse_link(&bytes, i + 1) {
                out.push_str(&format!("[image: {label}] {url}"));
                i = next;
                continue;
            }
        }
        if ch == '[' {
            if let Some((label, url, next)) = parse_link(&bytes, i) {
                if label.is_empty() {
                    out.push_str(&url);
                } else {
                    out.push_str(&format!("{label} ({url})"));
                }
                i = next;
                continue;
            }
        }
        if ch == '*' || ch == '`' {
            i += 1;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn parse_link(chars: &[char], open: usize) -> Option<(String, String, usize)> {
    let mut i = open + 1;
    let mut label = String::new();
    while i < chars.len() && chars[i] != ']' {
        label.push(chars[i]);
        i += 1;
    }
    if i + 1 >= chars.len() || chars[i] != ']' || chars[i + 1] != '(' {
        return None;
    }
    i += 2;
    let mut url = String::new();
    while i < chars.len() && chars[i] != ')' {
        url.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    Some((label, url, i + 1))
}
