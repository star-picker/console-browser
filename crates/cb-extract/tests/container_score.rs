//! FIX-4：container_score 必须真的有值，并能区分「抓对了」和「抓错了」。

use cb_extract::{extract_markdown, ExtractOpts};

fn md(html: &str) -> cb_extract::Markdown {
    extract_markdown(html, &ExtractOpts::default()).expect("extract")
}

fn score(html: &str) -> f64 {
    md(html)
        .container_score
        .expect("container_score must be present")
}

#[test]
fn article_page_scores_high_and_is_flagged_as_article() {
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
    let doc = md(&html);
    let s = doc.container_score.expect("score");
    assert!((0.7..=1.0).contains(&s), "article score {s} should be high");
    assert!(doc.is_article, "article container must be flagged");
}

#[test]
fn list_page_is_not_marked_as_an_article() {
    // 纯链接列表：容器就是整页，分数本身诚实；但要明确标注「不是文章页」。
    let mut links = String::new();
    for i in 0..60 {
        links.push_str(&format!(
            r#"<li><a href="https://example.com/{i}">item number {i}</a></li>"#
        ));
    }
    let html = format!(r#"<html><body><ul>{links}</ul></body></html>"#);
    let doc = md(&html);
    assert!(
        !doc.is_article,
        "link list page must not be reported as an article"
    );
    let s = doc.container_score.expect("score");
    assert!((0.0..=1.0).contains(&s));
}

#[test]
fn link_heavy_article_still_scores_high() {
    // 链接多的正文（周刊/新闻摘要）不是「抓错容器」。
    let mut links = String::new();
    for i in 0..20 {
        links.push_str(&format!(
            r#"<li><a href="https://example.com/{i}">a long enough link label {i}</a></li>"#
        ));
    }
    let html = format!(
        r#"<html><body><nav><a href="/">home</a></nav>
        <article><h1>Weekly</h1><p>周刊正文段落，长度足够让 article 被选中，不会退回 body 容器。</p><ul>{links}</ul></article>
        </body></html>"#
    );
    let s = score(&html);
    assert!(s >= 0.7, "link-heavy article score {s} should stay high");
}

#[test]
fn tiny_content_against_huge_noise_scores_low() {
    // 正文只有一小撮、页面其余是侧栏噪声：说明抓到的容器可疑。
    let article =
        "正文很短，但必须超过评分下限，这样才能被选中；它占整页的比例会很低，得分应当报警。"
            .repeat(3);
    let noise = "侧栏噪声文字，用来把页面总正文撑大，但它是 noise 容器不会进入正文。".repeat(40);
    let html = format!(
        r#"<html><body>
        <article><h1>T</h1><p>{article}</p></article>
        <div class="sidebar">{noise}</div>
        </body></html>"#
    );
    let s = score(&html);
    assert!(s < 0.3, "tiny content score {s} should be low");
}

#[test]
fn score_is_bounded_and_consistent() {
    let html = r#"<html><body><main><h1>T</h1><p>短正文，但足够让评分选中一个容器而不是空页，这里再多写一点点字。</p></main></body></html>"#;
    let s = score(html);
    assert!((0.0..=1.0).contains(&s), "score {s} out of range");
}
