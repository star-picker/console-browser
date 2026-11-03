//! FIX-4：container_score 必须真的有值，并能区分「抓对了」和「抓错了」。

use cb_extract::{extract_markdown, ExtractOpts};

fn score(html: &str) -> f64 {
    extract_markdown(html, &ExtractOpts::default())
        .expect("extract")
        .container_score
        .expect("container_score must be present")
}

#[test]
fn article_page_scores_high() {
    let body =
        "这是一篇真正的文章正文，内容足够长，确保主内容评分会选中 article 容器而不是外层包裹。"
            .repeat(4);
    let html = format!(
        r#"<html><body>
          <nav><a href="/a">a</a><a href="/b">b</a><a href="/c">c</a></nav>
          <article><h1>标题</h1><p>{body}</p></article>
          <footer><a href="/x">x</a></footer>
        </body></html>"#
    );
    let s = score(&html);
    assert!((0.7..=1.0).contains(&s), "article score {s} should be high");
}

#[test]
fn link_list_page_scores_low_or_is_reported_as_wrong_container() {
    // 纯链接列表：文本量很大，但链接密度接近 1，容器得分必须明显偏低。
    let mut links = String::new();
    for i in 0..60 {
        links.push_str(&format!(
            r#"<li><a href="https://example.com/{i}">item number {i}</a></li>"#
        ));
    }
    let html = format!(r#"<html><body><ul>{links}</ul></body></html>"#);
    let s = score(&html);
    assert!(s < 0.3, "list page score {s} should be low");
}

#[test]
fn score_is_bounded_and_consistent() {
    let html = r#"<html><body><main><h1>T</h1><p>短正文，但足够让评分选中一个容器而不是空页，这里再多写一点点字。</p></main></body></html>"#;
    let s = score(html);
    assert!((0.0..=1.0).contains(&s), "score {s} out of range");
}
