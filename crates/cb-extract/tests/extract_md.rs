use cb_extract::{extract_markdown, ExtractOpts};

fn fixture_dir(case: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(case)
}

fn extract(case: &str) -> cb_extract::Markdown {
    let dir = fixture_dir(case);
    let html = std::fs::read_to_string(dir.join("input.html")).expect("input.html");
    let expect = std::fs::read_to_string(dir.join("expect.md")).expect("expect.md");
    let md = extract_markdown(&html, &ExtractOpts::default()).expect("extract");
    assert_eq!(md.text, expect, "golden mismatch for {case}");
    md
}

#[test]
fn university_page_matches_golden() {
    let md = extract("university-page");
    assert!(md.links.iter().any(|l| l.url == "https://uni.example/faq"));
    assert!(md.tokens_estimate > 10);
    assert!(!md.truncated);
}

#[test]
fn github_repo_matches_golden() {
    let md = extract("github-repo");
    assert!(md.text.contains("```rust"), "code fence keeps its language");
}

#[test]
fn blog_with_nav_matches_golden() {
    let md = extract("blog-with-nav");
    assert!(md
        .images
        .iter()
        .any(|i| i.url == "https://img.example/a.png"));
    assert!(
        !md.text.contains("Buy stuff"),
        "sidebar ads must be dropped"
    );
}

#[test]
fn scope_selector_limits_output() {
    let dir = fixture_dir("blog-with-nav");
    let html = std::fs::read_to_string(dir.join("input.html")).expect("html");
    let opts = ExtractOpts {
        scope: Some("blockquote".to_string()),
        ..Default::default()
    };
    let md = extract_markdown(&html, &opts).expect("extract");
    assert_eq!(md.text.trim(), "> Measure twice.");
}

#[test]
fn max_tokens_truncates_and_flags() {
    let dir = fixture_dir("university-page");
    let html = std::fs::read_to_string(dir.join("input.html")).expect("html");
    let opts = ExtractOpts {
        max_tokens: Some(8),
        ..Default::default()
    };
    let md = extract_markdown(&html, &opts).expect("extract");
    assert!(md.truncated, "budget small enough must truncate");
    assert!(
        md.tokens_estimate <= 8,
        "estimate {} over budget",
        md.tokens_estimate
    );
    assert!(md.text.len() < 200);
}
#[test]
fn small_page_without_semantic_container_keeps_block_structure() {
    // 回归：没有 article/main 的小页面曾经被压成一行纯文本。
    let html = r#"<html><body><div class="wrap"><h1>Cache Rules</h1><p>See <a href="https://a.example/1">first</a>.</p><blockquote><p>Measure twice.</p></blockquote></div></body></html>"#;
    let md = extract_markdown(html, &ExtractOpts::default()).expect("extract");
    assert!(md.text.starts_with("# Cache Rules"), "got: {}", md.text);
    assert!(md.text.contains("> Measure twice."), "got: {}", md.text);
    assert!(
        md.text.contains("[first](https://a.example/1)"),
        "got: {}",
        md.text
    );
}

#[test]
fn whole_page_wrapper_is_not_mistaken_for_content() {
    // 回归：整页包裹 div 曾经赢过真正的语义容器。
    let html = r#"<html><body><div id="app"><nav><a href="/a">a</a><a href="/b">b</a><a href="/c">c</a></nav><article><h1>Real Article</h1><p>This is the actual body text that a reader wants and it is long enough to be selected by the scorer as the real content.</p></article></div></body></html>"#;
    let md = extract_markdown(html, &ExtractOpts::default()).expect("extract");
    assert!(md.text.starts_with("# Real Article"), "got: {}", md.text);
    assert!(
        !md.text.contains("[a](/a)"),
        "nav links leaked: {}",
        md.text
    );
}
