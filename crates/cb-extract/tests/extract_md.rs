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
