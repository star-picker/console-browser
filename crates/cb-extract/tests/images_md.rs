//! FIX-1：--to md 必须把图片按原始位置插回去，且 md 与 images[] 是同一份事实。

use cb_extract::{extract_markdown, ExtractOpts};

fn extract(html: &str) -> cb_extract::Markdown {
    extract_markdown(html, &ExtractOpts::default()).expect("extract")
}

fn md_images(text: &str) -> Vec<&str> {
    text.match_indices("![").map(|(i, _)| &text[i..]).collect()
}

#[test]
fn images_keep_document_order_and_empty_alt_falls_back_to_filename() {
    let html = r#"<html><body><main>
      <h1>带图文章</h1>
      <p>第一段：这张图后面还有正文，确保容器有足够字符被识别为正文内容而不是侧栏链接堆。</p>
      <img src="https://img.example.com/photo-1.webp">
      <p>第二段：图片应该在这一段之前、第一段之后，顺序不能乱，否则图和文对不上。</p>
      <figure><img src="https://img.example.com/photo-2.png" alt="流程图"><figcaption>图二说明</figcaption></figure>
    </main></body></html>"#;
    let md = extract(html);
    let text = &md.text;
    assert!(
        text.contains("![photo-1.webp](https://img.example.com/photo-1.webp)"),
        "{text}"
    );
    assert!(
        text.contains("![流程图](https://img.example.com/photo-2.png)"),
        "{text}"
    );
    assert!(
        text.contains("*图二说明*"),
        "figcaption must become italic caption: {text}"
    );

    let first_para = text.find("第一段").expect("first paragraph");
    let first_img = text.find("photo-1.webp").expect("first image");
    let second_para = text.find("第二段").expect("second paragraph");
    assert!(
        first_para < first_img,
        "image must come after its preceding paragraph:\n{text}"
    );
    assert!(
        first_img < second_para,
        "image must come before the next paragraph:\n{text}"
    );

    let figure_img = text.find("photo-2.png").expect("figure image");
    let caption = text.find("图二说明").expect("caption");
    assert!(
        figure_img < caption,
        "caption must follow its image:\n{text}"
    );
    assert_eq!(
        md.images.len(),
        2,
        "images[] must list the rendered images: {:?}",
        md.images
    );
    assert_eq!(
        md_images(text).len(),
        md.images.len(),
        "md and images[] disagree"
    );
}

#[test]
fn image_inside_link_is_not_dropped() {
    let html = r#"<html><body><article>
      <h1>Linked image</h1>
      <p>这段正文足够长，能通过主内容评分的最小长度门槛，不会被当作噪声丢掉，也不会选错容器。</p>
      <a href="https://example.com/target"><img src="https://img.example.com/banner.jpg"></a>
      <p>结尾段落。</p>
    </article></body></html>"#;
    let md = extract(html);
    assert!(
        md.text
            .contains("![banner.jpg](https://img.example.com/banner.jpg)"),
        "{}",
        md.text
    );
    assert_eq!(md.images.len(), 1);
    assert_eq!(
        md_images(&md.text).len(),
        md.images.len(),
        "linked image must not be counted twice:\n{}",
        md.text
    );
}

#[test]
fn image_links_do_not_duplicate_images_in_the_links_section() {
    // 链接数超过 inline 上限 → 追加 ## Links；图片标签不能在图后面再复制一遍。
    let mut links = String::new();
    for i in 0..35 {
        links.push_str(&format!(
            r#"<li><a href="https://example.com/link-{i}">link {i}</a></li>"#
        ));
    }
    let html = format!(
        r#"<html><body><article><h1>Links</h1>
        <p>这段正文足够长，可以让主内容评分选中 article，而不是落到整页包裹层或者链接列表上。</p>
        <p><a href="https://img.example.com/target"><img src="https://img.example.com/pic.png"></a></p>
        <ul>{links}</ul></article></body></html>"#
    );
    let md = extract(&html);
    assert!(md.text.contains("## Links"), "{}", md.text);
    assert_eq!(
        md_images(&md.text).len(),
        md.images.len(),
        "images[] and markdown must agree:\n{}",
        md.text
    );
}

#[test]
fn no_images_switch_suppresses_markdown_but_keeps_metadata() {
    let html = r#"<html><body><article>
      <h1>图集</h1>
      <p>正文足够长，足以让主内容评分选中 article 容器，不会落到 body 包裹层上，也不会被噪声过滤掉。</p>
      <img src="https://img.example.com/a.png">
      <img src="https://img.example.com/b.png" alt="b">
    </article></body></html>"#;
    let opts = ExtractOpts {
        images: false,
        ..Default::default()
    };
    let md = extract_markdown(html, &opts).expect("extract");
    assert!(
        !md.text.contains("!["),
        "no markdown images expected:\n{}",
        md.text
    );
    assert_eq!(md.images.len(), 2, "metadata must still list images");
}

#[test]
fn truncation_keeps_images_even_when_text_is_cut() {
    let long = "很长的一段正文，用来把 token 预算撑爆，这样截断逻辑必须做取舍。".repeat(20);
    let html = format!(
        r#"<html><body><article><h1>预算</h1><p>{long}</p><img src="https://img.example.com/keep.png"><p>{long}</p></article></body></html>"#
    );
    let opts = ExtractOpts {
        max_tokens: Some(30),
        ..Default::default()
    };
    let md = extract_markdown(&html, &opts).expect("extract");
    assert!(md.truncated, "budget must truncate");
    assert!(
        md.text
            .contains("![keep.png](https://img.example.com/keep.png)"),
        "images must survive truncation:\n{}",
        md.text
    );
    assert!(md.images.iter().any(|i| i.url.ends_with("keep.png")));
}

#[test]
fn md_image_count_equals_images_array_on_fixture() {
    let html = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/blog-with-nav/input.html"),
    )
    .expect("fixture");
    let md = extract(&html);
    assert!(!md.images.is_empty());
    assert_eq!(md_images(&md.text).len(), md.images.len());
}
