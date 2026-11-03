//! HTML → RenderDoc（结构化、带链接 id），供 TUI 渲染。
//! 语义与 markdown 链路保持一致：同样的正文容器评分、同样的噪声过滤、同样的行内样式。

use crate::blocks::{Block, LinkRef, NoticeLevel, RenderDoc, Span, SpanStyle};
use crate::ExtractOpts;
use cb_dom::Document;
use scraper::{ElementRef, Selector};

struct Ctx {
    links: Vec<LinkRef>,
    images: Vec<(String, String)>,
    semantic_root: bool,
}

fn sel(css: &str) -> Option<Selector> {
    Selector::parse(css).ok()
}

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

fn push_text(spans: &mut Vec<Span>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.style == SpanStyle::Normal && last.link.is_none() {
            last.text.push_str(text);
            return;
        }
    }
    spans.push(Span::normal(text));
}

fn span_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn inline_node(ctx: &mut Ctx, node: ego_tree::NodeRef<'_, scraper::Node>, spans: &mut Vec<Span>) {
    match node.value() {
        scraper::Node::Text(t) => push_text(spans, &collapse_keep_edges(&t.text)),
        scraper::Node::Element(_) => {
            let el = ElementRef::wrap(node).expect("element");
            if cb_dom::is_noise(&el) {
                return;
            }
            match el.value().name() {
                "strong" | "b" => {
                    let mut inner = Vec::new();
                    inline_children(ctx, el, &mut inner);
                    let text = span_text(&inner);
                    let t = text.trim();
                    if !t.is_empty() {
                        spans.push(Span {
                            text: t.to_string(),
                            style: SpanStyle::Bold,
                            link: None,
                        });
                    }
                }
                "em" | "i" => {
                    let mut inner = Vec::new();
                    inline_children(ctx, el, &mut inner);
                    let text = span_text(&inner);
                    let t = text.trim();
                    if !t.is_empty() {
                        spans.push(Span {
                            text: t.to_string(),
                            style: SpanStyle::Italic,
                            link: None,
                        });
                    }
                }
                "code" => {
                    let t: String = el.text().collect();
                    if !t.trim().is_empty() {
                        spans.push(Span {
                            text: t.trim().to_string(),
                            style: SpanStyle::Code,
                            link: None,
                        });
                    }
                }
                "a" => {
                    let href = el
                        .value()
                        .attr("href")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let mut inner = Vec::new();
                    inline_children(ctx, el, &mut inner);
                    let text = span_text(&inner);
                    let label = text.trim().to_string();
                    if label.is_empty() {
                        return;
                    }
                    if href.is_empty() {
                        push_text(spans, &label);
                        return;
                    }
                    let id = ctx.links.len() as u32;
                    // 相邻链接挤在一起会变成「新闻hao123地图」，先补个空格再放链接。
                    if let Some(last) = spans.last() {
                        if !last
                            .text
                            .ends_with(|c: char| c.is_whitespace() || "|，。、；：".contains(c))
                        {
                            push_text(spans, " ");
                        }
                    }
                    ctx.links.push(LinkRef {
                        id,
                        url: href,
                        text: label.clone(),
                    });
                    spans.push(Span {
                        text: label,
                        style: SpanStyle::Normal,
                        link: Some(id),
                    });
                }
                "img" => {
                    let Some((src, _lazy)) = crate::best_image_src(el) else {
                        return;
                    };
                    let alt = el
                        .value()
                        .attr("alt")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    ctx.images.push((alt.clone(), src.clone()));
                    // 没 alt 的图片是装饰/图标，阅读视图里不占位置。
                    if alt.is_empty() {
                        return;
                    }
                    push_text(spans, &format!("[img: {}] {}", alt, src));
                }
                "br" => push_text(spans, " "),
                _ => inline_children(ctx, el, spans),
            }
        }
        _ => {}
    }
}

fn inline_children(ctx: &mut Ctx, el: ElementRef<'_>, spans: &mut Vec<Span>) {
    for child in el.children() {
        inline_node(ctx, child, spans);
    }
}

fn inline_spans(ctx: &mut Ctx, el: ElementRef<'_>) -> Vec<Span> {
    let mut spans = Vec::new();
    inline_children(ctx, el, &mut spans);
    trim_spans(spans)
}

fn trim_spans(mut spans: Vec<Span>) -> Vec<Span> {
    if let Some(first) = spans.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = spans.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    spans.retain(|s| !s.text.is_empty());
    spans
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

const LIST_TAGS: &[&str] = &["ul", "ol"];

/// 块级标签真集合：子树里有这些就直接递归，否则整块当行内段落。
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

/// 形态像 JSON 数据转储（Baidu/头条这类站会把数据塞进可见文本）。
/// 这不是正文，别塞进阅读视图或 AI 上下文。
fn looks_like_json_dump(text: &str) -> bool {
    let t = text.trim();
    if t.chars().count() < 120 {
        return false;
    }
    let pairs = t.matches("\":").count() + t.matches("\": ").count();
    let brackets = t.matches('{').count() + t.matches('[').count();
    pairs >= 3 && brackets >= 1
}

fn walk_children(ctx: &mut Ctx, el: ElementRef<'_>, depth: u8, blocks: &mut Vec<Block>) {
    for child in el.children() {
        if let Some(child_el) = ElementRef::wrap(child) {
            walk_element(ctx, child_el, depth, blocks);
        }
    }
}

fn walk_element(ctx: &mut Ctx, el: ElementRef<'_>, depth: u8, blocks: &mut Vec<Block>) {
    if cb_dom::is_noise(&el) {
        return;
    }
    if cb_dom::is_page_chrome(&el) && !ctx.semantic_root {
        return;
    }
    let name = el.value().name();
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = name[1..].parse::<u8>().unwrap_or(1);
            let spans = inline_spans(ctx, el);
            if !spans.is_empty() {
                blocks.push(Block::Heading { level, spans });
            }
        }
        "body" | "main" | "article" => walk_children(ctx, el, depth, blocks),
        "p" | "div" | "section" | "figure" | "details" | "dl" => {
            if name == "p" || !has_block_children(el) {
                let spans = inline_spans(ctx, el);
                if !spans.is_empty() && !looks_like_json_dump(&span_text(&spans)) {
                    blocks.push(Block::Paragraph { spans });
                }
            } else {
                walk_children(ctx, el, depth, blocks);
            }
        }
        "pre" => {
            let lang = code_fence_language(el);
            let code = sel("code").and_then(|s| el.select(&s).next()).unwrap_or(el);
            let body = code.text().collect::<String>();
            let body = body.trim_end_matches('\n').to_string();
            let lines: Vec<String> = body.split('\n').map(|l| l.to_string()).collect();
            blocks.push(Block::Code { lang, lines });
        }
        "blockquote" => {
            let mut inner = Vec::new();
            walk_children(ctx, el, depth, &mut inner);
            if !inner.is_empty() {
                blocks.push(Block::Quote { blocks: inner });
            }
        }
        "table" => {
            if let Some(block) = table_block(el) {
                blocks.push(block);
            }
        }
        "ul" | "ol" => list_blocks(ctx, el, depth, blocks),
        "hr" => blocks.push(Block::Rule),
        _ => {
            if LIST_TAGS.contains(&name) {
                return;
            }
            let spans = inline_spans(ctx, el);
            if !spans.is_empty() && !looks_like_json_dump(&span_text(&spans)) {
                blocks.push(Block::Paragraph { spans });
            }
        }
    }
}

fn list_blocks(ctx: &mut Ctx, list: ElementRef<'_>, depth: u8, out: &mut Vec<Block>) {
    let ordered = list.value().name() == "ol";
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
            format!("{}. ", index)
        } else {
            "- ".to_string()
        };
        let mut spans = Vec::new();
        let mut nested: Vec<Block> = Vec::new();
        for child in li.children() {
            if let Some(child_el) = ElementRef::wrap(child) {
                if LIST_TAGS.contains(&child_el.value().name()) {
                    list_blocks(ctx, child_el, depth + 1, &mut nested);
                    continue;
                }
            }
            inline_node(ctx, child, &mut spans);
        }
        let spans = trim_spans(spans);
        if spans.is_empty() && nested.is_empty() {
            continue;
        }
        if !spans.is_empty() {
            out.push(Block::ListItem {
                depth,
                marker,
                spans,
            });
        }
        out.extend(nested);
    }
}

fn table_block(table: ElementRef<'_>) -> Option<Block> {
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
            cells.push(collapse_keep_edges(&text).trim().to_string());
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let head = rows.first().cloned().unwrap_or_default();
    let body = rows.into_iter().skip(1).collect();
    Some(Block::Table { head, rows: body })
}

/// 把 HTML 变成可渲染文档。范围/预算语义与 extract_markdown 一致。
pub fn to_document(html: &str, opts: &ExtractOpts) -> anyhow::Result<RenderDoc> {
    let doc = Document::parse(html);
    let (root, semantic_root) = match &opts.scope {
        Some(css) => (
            doc.select(css)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("scope selector matched nothing: {}", css))?,
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

    let mut ctx = Ctx {
        links: Vec::new(),
        images: Vec::new(),
        semantic_root,
    };
    let mut blocks = Vec::new();
    match root.value().name() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "pre" | "blockquote" | "table" | "ul"
        | "ol" | "hr" => walk_element(&mut ctx, root, 0, &mut blocks),
        _ => walk_children(&mut ctx, root, 0, &mut blocks),
    }

    for (alt, url) in ctx.images.clone() {
        let already = blocks.iter().any(|b| match b {
            Block::Image { url: u, .. } => u == &url,
            _ => false,
        });
        if !already && !alt.is_empty() {
            blocks.push(Block::Image { alt, url });
        }
    }

    let title = doc.title().unwrap_or_default();
    Ok(RenderDoc {
        title,
        blocks,
        links: ctx.links,
    })
}

/// 给 TUI 的合成页面用：错误页、空状态、搜索结果页。
pub fn notice_doc(
    title: impl Into<String>,
    level: NoticeLevel,
    message: impl Into<String>,
) -> RenderDoc {
    let title = title.into();
    RenderDoc {
        title: title.clone(),
        blocks: vec![Block::Notice {
            level,
            title,
            spans: vec![Span::normal(message.into())],
        }],
        links: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExtractOpts;

    fn doc(html: &str) -> RenderDoc {
        to_document(html, &ExtractOpts::default()).expect("doc")
    }

    #[test]
    fn headings_paragraphs_and_links_get_stable_ids() {
        let html = r#"<html><body><article><h1>Title</h1><p>See <a href="https://a/1">one</a> and <a href="https://a/2">two</a>.</p></article></body></html>"#;
        let d = doc(html);
        assert!(matches!(d.blocks[0], Block::Heading { level: 1, .. }));
        assert_eq!(d.links.len(), 2);
        assert_eq!(d.links[0].id, 0);
        assert_eq!(d.links[0].url, "https://a/1");
        assert_eq!(d.links[1].id, 1);
        let para = match &d.blocks[1] {
            Block::Paragraph { spans } => spans,
            other => panic!("expected paragraph, got {other:?}"),
        };
        let linked: Vec<u32> = para.iter().filter_map(|s| s.link).collect();
        assert_eq!(linked, vec![0, 1]);
    }

    #[test]
    fn nested_lists_become_depth_blocks() {
        let html = r#"<article><ul><li>a<ul><li>b</li></ul></li><li>c</li></ul></article>"#;
        let d = doc(html);
        let items: Vec<(u8, String)> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::ListItem { depth, marker, .. } => Some((*depth, marker.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, "- ".to_string()),
                (1, "- ".to_string()),
                (0, "- ".to_string())
            ]
        );
    }

    #[test]
    fn code_blocks_keep_language_and_lines() {
        let html =
            r#"<article><pre><code class="language-rust">fn main() {</code></pre></article>"#;
        let d = doc(html);
        match &d.blocks[0] {
            Block::Code { lang, lines } => {
                assert_eq!(lang, "rust");
                assert_eq!(lines.len(), 1);
                assert!(lines[0].starts_with("fn main"));
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn tables_split_head_and_rows() {
        let html = r#"<article><table><thead><tr><th>K</th><th>V</th></tr></thead><tbody><tr><td>a</td><td>1</td></tr></tbody></table></article>"#;
        let d = doc(html);
        match &d.blocks[0] {
            Block::Table { head, rows } => {
                assert_eq!(head, &vec!["K".to_string(), "V".to_string()]);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0], vec!["a".to_string(), "1".to_string()]);
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn inline_styles_are_spans_not_markup() {
        let html = r#"<article><p>a <strong>bold</strong> and <em>it</em> and <code>c</code></p></article>"#;
        let d = doc(html);
        let spans = match &d.blocks[0] {
            Block::Paragraph { spans } => spans,
            other => panic!("expected paragraph, got {other:?}"),
        };
        let styles: Vec<SpanStyle> = spans.iter().map(|s| s.style.clone()).collect();
        assert!(styles.contains(&SpanStyle::Bold));
        assert!(styles.contains(&SpanStyle::Italic));
        assert!(styles.contains(&SpanStyle::Code));
    }

    #[test]
    fn notice_doc_is_renderable() {
        let d = notice_doc("Oops", NoticeLevel::Error, "something failed");
        assert_eq!(d.blocks.len(), 1);
        assert!(d.text_len() > 0);
    }
}
