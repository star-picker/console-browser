//! P1: 懒加载图片属性链 + 占位图判定（微信/知乎/简书/Medium 通用）。

use cb_extract::{extract_markdown, ExtractOpts};

#[test]
fn picks_real_url_from_lazy_attributes_and_srcset() {
    let html = r#"<main>
      <p>正文足够长，确保主内容评分选中这里而不是侧栏。这是一段真实存在的文章内容，用来让容器评分有足够密度。</p>
      <img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" data-src="https://img.example.com/real-1.jpg" alt="图一">
      <img src="https://x/pixel.gif" data-original="https://img.example.com/real-2.jpg" alt="图二">
      <img srcset="https://img.example.com/s.jpg 320w, https://img.example.com/l.jpg 1600w" src="https://img.example.com/s.jpg" alt="图三">
      <img src="https://img.example.com/plain.jpg" alt="图四">
    </main>"#;
    let md = extract_markdown(html, &ExtractOpts::default()).expect("md");
    let urls: Vec<&str> = md.images.iter().map(|i| i.url.as_str()).collect();
    assert!(
        urls.contains(&"https://img.example.com/real-1.jpg"),
        "{urls:?}"
    );
    assert!(
        urls.contains(&"https://img.example.com/real-2.jpg"),
        "{urls:?}"
    );
    assert!(
        urls.contains(&"https://img.example.com/l.jpg"),
        "srcset largest: {urls:?}"
    );
    assert!(
        urls.contains(&"https://img.example.com/plain.jpg"),
        "{urls:?}"
    );
    assert!(
        !urls.iter().any(|u| u.contains("pixel.gif")),
        "placeholder leaked: {urls:?}"
    );
    assert!(
        !urls.iter().any(|u| u.starts_with("data:")),
        "data uri leaked: {urls:?}"
    );
    // 正文里能出现 alt 文案即可（图片是否内联成 markdown 取决于层级，不在此断言）。
    assert!(
        md.images.iter().any(|i| i.lazy && i.url.contains("real-1")),
        "lazy flag must be set"
    );
}

#[test]
fn placeholder_only_image_is_skipped_entirely() {
    let html = r#"<main>
      <p>同样的正文长度门槛，确保这段文字被当正文，而不是被当成噪声丢掉的一段话。</p>
      <img src="https://x/placeholder.png" width="1" height="1" alt="占位">
    </main>"#;
    let md = extract_markdown(html, &ExtractOpts::default()).expect("md");
    assert!(md.images.is_empty(), "{md:?}");
    assert!(!md.text.contains("placeholder.png"), "{}", md.text);
}
