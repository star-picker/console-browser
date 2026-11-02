//! 回归：门户式页面（容器套容器 + 内嵌 JSON + 图标图）不得被压成一行。

const PORTAL: &str = r#"<html><body><div id="wrap">
  <div class="nav"><a href="/a">新闻</a><a href="/b">地图</a></div>
  <div class="icons"><img src="https://x/i1.png"><img src="https://x/i2.png"></div>
  <div class="data">{ "hotsearch": [ { "card_title": "x", "views": "1", "heat_score": "2" }, { "card_title": "y", "views": "3", "heat_score": "4" } ] }</div>
  <div class="content"><h2>真正的标题</h2><p>真正的正文段落，应该被保留下来并且单独成段。</p></div>
</div></body></html>"#;

use cb_extract::{extract_markdown, to_document, ExtractOpts};

#[test]
fn document_keeps_structure_and_drops_json_and_icon_images() {
    let doc = to_document(PORTAL, &ExtractOpts::default()).expect("doc");
    let text: String = doc
        .blocks
        .iter()
        .map(|b| format!("{b:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("真正的正文段落"), "body text missing: {text}");
    assert!(text.contains("真正的标题"), "heading missing: {text}");
    assert!(!text.contains("card_title"), "json dump leaked: {text}");
    assert!(!text.contains("i1.png"), "empty-alt icon leaked: {text}");
}

#[test]
fn markdown_keeps_structure_and_drops_json_and_icon_images() {
    let md = extract_markdown(PORTAL, &ExtractOpts::default()).expect("md");
    assert!(md.text.contains("真正的正文段落"), "{}", md.text);
    assert!(md.text.contains("## 真正的标题"), "{}", md.text);
    assert!(!md.text.contains("card_title"), "{}", md.text);
    assert!(
        !md.text.contains("[](https://x/i1.png)") && !md.text.contains("![]("),
        "{}",
        md.text
    );
    assert!(
        md.images.len() >= 2,
        "images still collected for AI: {}",
        md.images.len()
    );
}
