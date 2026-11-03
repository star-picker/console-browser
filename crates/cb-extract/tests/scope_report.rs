//! FIX-7：--scope 命中与否必须回执，调用方能区分「命中但没缩小」和「没命中」。

use cb_extract::{extract_markdown, ExtractOpts};

fn fixture(case: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(case)
            .join("input.html"),
    )
    .expect("fixture")
}

#[test]
fn matched_scope_that_shrinks_reports_both_sizes() {
    let html = fixture("blog-with-nav");
    let opts = ExtractOpts {
        scope: Some("blockquote".to_string()),
        ..Default::default()
    };
    let md = extract_markdown(&html, &opts).expect("extract");
    let scope = md.scope.expect("scope receipt");
    assert!(scope.matched);
    assert_eq!(scope.selector, "blockquote");
    let before = scope.chars_before.expect("chars_before");
    let after = scope.chars_after.expect("chars_after");
    assert!(
        after < before,
        "narrowed scope must shrink: {before} -> {after}"
    );
    assert_eq!(after, md.text.chars().count());
}

#[test]
fn matched_scope_that_does_not_shrink_is_still_matched() {
    // 只有 body 一层容器：默认提取与 --scope body 选中的是同一个容器。
    let html = r#"<html><body><h1>标题</h1><p>正文足够长，足以让主内容评分退回 body 作为内容容器，这样默认路径与 scope 路径一致，前后字符数也应该一致。</p></body></html>"#;
    let opts = ExtractOpts {
        scope: Some("body".to_string()),
        ..Default::default()
    };
    let md = extract_markdown(html, &opts).expect("extract");
    let scope = md.scope.expect("scope receipt");
    assert!(scope.matched);
    assert_eq!(
        scope.chars_before, scope.chars_after,
        "same container: before == after"
    );
}

#[test]
fn unmatched_scope_is_not_an_error_but_says_so() {
    let html = fixture("university-page");
    let opts = ExtractOpts {
        scope: Some(".does-not-exist".to_string()),
        ..Default::default()
    };
    let md = extract_markdown(&html, &opts).expect("unmatched scope must not be an error");
    let scope = md.scope.expect("scope receipt");
    assert!(!scope.matched, "must report matched=false");
    assert_eq!(scope.chars_after, Some(0));
    assert!(scope.chars_before.unwrap_or(0) > 0);
    assert!(md.text.is_empty());
}

#[test]
fn no_scope_means_no_receipt() {
    let html = fixture("university-page");
    let md = extract_markdown(&html, &ExtractOpts::default()).expect("extract");
    assert!(md.scope.is_none());
}
