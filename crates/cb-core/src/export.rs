//! 导出：单文件 HTML（自包含、可离线阅读、无脚本）。

use crate::CbError;

/// 极简阅读样式：单文件 HTML 自带，不依赖外部资源。
const READER_CSS: &str = r#"
:root { color-scheme: light dark; }
body { max-width: 44rem; margin: 2rem auto; padding: 0 1rem;
  font: 16px/1.65 -apple-system, "Segoe UI", system-ui, sans-serif; }
pre, code { font-family: ui-monospace, Consolas, monospace; font-size: 0.92em; }
pre { background: rgba(127,127,127,0.12); padding: 0.8rem; overflow-x: auto; border-radius: 6px; }
code { background: rgba(127,127,127,0.12); padding: 0.1em 0.3em; border-radius: 3px; }
img { max-width: 100%; height: auto; }
table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
th, td { border: 1px solid rgba(127,127,127,0.35); padding: 0.4rem 0.6rem; text-align: left; }
blockquote { margin: 1rem 0; padding-left: 0.8rem; border-left: 3px solid rgba(127,127,127,0.4); }
a { color: #0a7dc2; }
footer { margin-top: 3rem; font-size: 0.85em; opacity: 0.7; }
"#;

fn re(pattern: &str) -> regex::Regex {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .expect("static regex")
}

/// 把页面变成单文件 HTML：去掉脚本与内联事件、去掉外部样式表，注入阅读样式。
pub fn single_file_html(raw: &str, title: &str, source_url: &str) -> String {
    let mut html = raw.to_string();

    html = re(r"<script\b[^>]*>.*?</script\s*>")
        .replace_all(&html, "")
        .to_string();
    html = re(r"<script\b[^>]*/?>").replace_all(&html, "").to_string();
    html = re(r"<noscript\b[^>]*>.*?</noscript\s*>")
        .replace_all(&html, "")
        .to_string();
    html = re(r"<link\b[^>]*>").replace_all(&html, "").to_string();
    html = re(r#"(?i)\s+on[a-z]+\s*=\s*"[^"]*""#)
        .replace_all(&html, "")
        .to_string();
    html = re(r"(?i)\s+on[a-z]+\s*=\s*'[^']*'")
        .replace_all(&html, "")
        .to_string();

    let footer = format!(
        "<footer>source: <a href=\"{url}\">{url}</a></footer>",
        url = html_escape(source_url)
    );
    let style = format!("<style>{READER_CSS}</style>");
    let safe_title = html_escape(if title.is_empty() { source_url } else { title });

    // 传入的 title 是权威的：已有的 <title> 直接替换。
    let title_re = re(r"<title\b[^>]*>.*?</title\s*>");
    html = title_re
        .replace_all(&html, format!("<title>{safe_title}</title>"))
        .to_string();
    let head_inject = format!(
        "{style}{title_tag}",
        title_tag = title_tag_for(&html, &safe_title)
    );
    if let Some(pos) = html.to_ascii_lowercase().find("</head>") {
        html.insert_str(pos, &head_inject);
    } else {
        // 没有 head 的碎片 HTML：自己补一个，保证单文件可读。
        html = format!("<html><head>{head_inject}</head><body>{html}</body></html>");
    }
    if let Some(pos) = html.to_ascii_lowercase().rfind("</body>") {
        html.insert_str(pos, &footer);
    } else {
        html.push_str(&footer);
    }
    html
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 导出到文件或返回内容。
pub fn write_or_return(out: Option<&std::path::Path>, content: &str) -> Result<(), CbError> {
    match out {
        Some(path) => std::fs::write(path, content)
            .map_err(|e| CbError::other(format!("write {}: {e}", path.display()))),
        None => Ok(()),
    }
}

fn title_tag_for(html: &str, safe_title: &str) -> String {
    if html.to_ascii_lowercase().contains("<title") {
        String::new()
    } else {
        format!("<title>{safe_title}</title>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><head><title>Old</title><link rel="stylesheet" href="x.css"></head><body><h1>Hi</h1><script>var a=1;</script><p onclick="steal()">text</p></body></html>"#;

    #[test]
    fn strips_scripts_links_and_inline_handlers() {
        let out = single_file_html(PAGE, "New", "https://example.com/a");
        assert!(!out.contains("var a=1"), "script body must go: {out}");
        assert!(!out.contains("<script"), "script tag must go");
        assert!(!out.contains("onclick"), "inline handler must go");
        assert!(!out.contains("x.css"), "external stylesheet must go");
    }

    #[test]
    fn injects_reader_style_title_and_source_footer() {
        let out = single_file_html(PAGE, "New Title", "https://example.com/a");
        assert!(out.contains("<style>"), "reader css injected");
        assert!(out.contains("New Title"), "title kept");
        assert!(out.contains("https://example.com/a"), "source footer");
        assert!(out.contains("<h1>Hi</h1>"), "content survives");
    }

    #[test]
    fn escapes_title_and_url() {
        let out = single_file_html(
            "<html><body>x</body></html>",
            "<bad>",
            "https://e.com/?a=1&b=2",
        );
        assert!(out.contains("&lt;bad&gt;"));
        assert!(out.contains("a=1&amp;b=2"));
    }
}
