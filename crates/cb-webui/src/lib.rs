//! cb-webui: 本地 Web 镜像 —— 同一份阅读视图，用 SSE 推给浏览器。
//!
//! 设计取舍：不引入前端构建链，一个内嵌 HTML + EventSource 就够；
//! 状态只有一份（最近一次快照），推送给所有连着的页面。

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use cb_extract::{Block, RenderDoc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 一行阅读视图：kind 决定前端用什么样式渲染。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineView {
    pub text: String,
    pub kind: String,
    /// kind == "img" 时的图片地址（webui 能真显示图，终端不能）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// 推给前端的一帧：地址栏 + 标题 + 状态 + 正文行。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    pub status: String,
    pub lines: Vec<LineView>,
}

#[derive(Clone)]
pub struct WebState {
    snapshot: Arc<RwLock<Snapshot>>,
    ctx: cb_core::Ctx,
}

impl WebState {
    pub fn new(ctx: cb_core::Ctx) -> WebState {
        WebState {
            snapshot: Arc::new(RwLock::new(Snapshot::default())),
            ctx,
        }
    }

    pub async fn set(&self, snapshot: Snapshot) {
        *self.snapshot.write().await = snapshot;
    }

    pub async fn current(&self) -> Snapshot {
        self.snapshot.read().await.clone()
    }

    pub fn ctx(&self) -> &cb_core::Ctx {
        &self.ctx
    }
}

/// 把 RenderDoc 拍平成前端友好的行（标题/段落/列表/代码/引用/提示）。
pub fn lines_from_doc(doc: &RenderDoc) -> Vec<LineView> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        match block {
            Block::Heading { level, spans } => out.push(LineView {
                text: spans_text(spans),
                kind: format!("h{}", (*level).clamp(1, 6)),
                url: None,
            }),
            Block::Paragraph { spans } => out.push(LineView {
                text: spans_text(spans),
                kind: "p".to_string(),
                url: None,
            }),
            Block::ListItem {
                depth,
                marker,
                spans,
            } => out.push(LineView {
                text: format!(
                    "{}{} {}",
                    "  ".repeat(*depth as usize),
                    marker,
                    spans_text(spans)
                ),
                kind: "li".to_string(),
                url: None,
            }),
            Block::Code { lang, lines } => {
                let mut text = lines.join("\n");
                if !lang.is_empty() {
                    text = format!("[{lang}]\n{text}");
                }
                out.push(LineView {
                    text,
                    kind: "pre".to_string(),
                    url: None,
                });
            }
            Block::Quote { blocks } => {
                let inner = blocks.iter().map(line_of).collect::<Vec<_>>().join(" ");
                out.push(LineView {
                    text: inner,
                    kind: "quote".to_string(),
                    url: None,
                });
            }
            Block::Table { head, rows } => {
                let mut text = head.join(" | ");
                for row in rows {
                    text.push('\n');
                    text.push_str(&row.join(" | "));
                }
                out.push(LineView {
                    text,
                    kind: "table".to_string(),
                    url: None,
                });
            }
            Block::Rule => out.push(LineView {
                text: String::new(),
                kind: "hr".to_string(),
                url: None,
            }),
            Block::Image { alt, url } => out.push(LineView {
                text: if alt.trim().is_empty() {
                    "图片".to_string()
                } else {
                    alt.clone()
                },
                kind: "img".to_string(),
                url: Some(url.clone()),
            }),
            Block::Notice {
                level,
                title,
                spans,
            } => out.push(LineView {
                text: format!(
                    "[{}] {} {}",
                    format!("{level:?}").to_lowercase(),
                    title,
                    spans_text(spans)
                ),
                kind: "notice".to_string(),
                url: None,
            }),
        }
    }
    out
}

fn line_of(block: &Block) -> String {
    match block {
        Block::Paragraph { spans } | Block::Heading { spans, .. } => spans_text(spans),
        Block::ListItem { spans, .. } => spans_text(spans),
        Block::Code { lines, .. } => lines.join(" "),
        _ => String::new(),
    }
}

fn spans_text(spans: &[cb_extract::Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/state", get(state_json))
        .route("/api/navigate", post(navigate))
        .route("/events", get(events))
        .with_state(state)
}

#[derive(Debug, Default, Deserialize)]
pub struct IndexQuery {
    pub url: Option<String>,
}

async fn index(Query(q): Query<IndexQuery>) -> Html<String> {
    Html(index_html(q.url.as_deref()))
}

/// 首页 HTML；带 ?url= 时把起始地址注入页面，前端加载后自动 navigate。
pub fn index_html(start: Option<&str>) -> String {
    let Some(start) = start.map(str::trim).filter(|u| !u.is_empty()) else {
        return INDEX_HTML.to_string();
    };
    // 注入 JS 字符串：转义 HTML 敏感字符，防止 URL 提前闭合 script。
    let mut js = serde_json::to_string(start).unwrap_or_else(|_| "\"\"".to_string());
    js = js
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let injected =
        format!("<script>window.__CB_START_URL__ = {js};</script>\n<script>\nconst START_URL");
    INDEX_HTML.replace("<script>\nconst START_URL", &injected)
}

async fn state_json(State(state): State<WebState>) -> Json<Snapshot> {
    Json(state.current().await)
}

#[derive(Deserialize)]
pub struct NavigateReq {
    pub url: String,
}

async fn navigate(State(state): State<WebState>, Json(req): Json<NavigateReq>) -> Json<Snapshot> {
    let snapshot = load_snapshot(state.ctx(), &req.url).await;
    let snapshot = match snapshot {
        Ok(s) => s,
        Err(e) => Snapshot {
            url: req.url.clone(),
            title: req.url.clone(),
            status: format!("error: {e}"),
            lines: Vec::new(),
        },
    };
    state.set(snapshot.clone()).await;
    Json(snapshot)
}

/// 抓一个页面并拍成快照（CLI 的 `cb webui --url` 和网页导航共用）。
pub async fn load_snapshot(ctx: &cb_core::Ctx, url: &str) -> anyhow::Result<Snapshot> {
    let opts = cb_core::FetchOpts {
        url: url.to_string(),
        ..Default::default()
    };
    let outcome = cb_core::fetch_page(ctx, &opts).await?;
    let lines = match &outcome.doc {
        Some(doc) => lines_from_doc(doc),
        None => Vec::new(),
    };
    if lines.is_empty() {
        return Ok(Snapshot {
            url: outcome.final_url.clone(),
            title: outcome.title.clone(),
            status: "no content".to_string(),
            lines: vec![LineView {
                text: outcome.markdown.clone(),
                kind: "pre".to_string(),
                url: None,
            }],
        });
    }
    Ok(Snapshot {
        url: outcome.final_url.clone(),
        title: outcome.title.clone(),
        status: if outcome.cached { "cached" } else { "fetched" }.to_string(),
        lines,
    })
}

async fn events(
    State(state): State<WebState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let stream = futures_util::stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let snapshot = state.current().await;
        let data = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
        Some((Ok(Event::default().event("snapshot").data(data)), state))
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// 起服务；返回实际监听地址（端口 0 时由系统分配）。
pub async fn serve(
    ctx: cb_core::Ctx,
    port: u16,
    start_url: Option<String>,
) -> anyhow::Result<String> {
    let state = WebState::new(ctx);
    if let Some(url) = start_url.clone() {
        if let Ok(snapshot) = load_snapshot(state.ctx(), &url).await {
            state.set(snapshot).await;
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(format!("http://{addr}/"))
}

pub const INDEX_HTML: &str = include_str!("index.html");

#[cfg(test)]
mod tests {
    use super::*;
    use cb_extract::{ExtractOpts, Span, SpanStyle};

    fn doc(html: &str) -> RenderDoc {
        cb_extract::to_document(html, &ExtractOpts::default()).expect("doc")
    }

    #[test]
    fn lines_keep_browser_like_kinds() {
        let html = r#"<main><h1>标题一</h1><p>正文段落。</p><ul><li>要点</li></ul><pre><code>fn f() {}</code></pre></main>"#;
        let lines = lines_from_doc(&doc(html));
        assert!(lines
            .iter()
            .any(|l| l.kind == "h1" && l.text.contains("标题一")));
        assert!(lines
            .iter()
            .any(|l| l.kind == "p" && l.text.contains("正文段落")));
        assert!(lines
            .iter()
            .any(|l| l.kind == "li" && l.text.contains("要点")));
        assert!(lines
            .iter()
            .any(|l| l.kind == "pre" && l.text.contains("fn f")));
    }

    #[test]
    fn index_is_self_contained_and_uses_sse() {
        assert!(INDEX_HTML.contains("EventSource"), "must stream via SSE");
        assert!(INDEX_HTML.contains("/api/navigate"));
        assert!(!INDEX_HTML.contains("http://cdn"), "no external assets");
    }

    #[test]
    fn index_without_url_stays_empty() {
        let html = index_html(None);
        assert_eq!(html, INDEX_HTML);
        assert!(
            !html.contains("window.__CB_START_URL__ ="),
            "no start url assignment when the query is absent"
        );
    }

    #[test]
    fn index_with_url_injects_start_url_and_auto_navigate() {
        let html = index_html(Some("https://example.com/a?x=1&y=2"));
        assert!(
            html.contains("window.__CB_START_URL__ = "),
            "start url must be injected"
        );
        assert!(html.contains("https://example.com/a?x=1"), "{html}");
        assert!(html.contains("if (START_URL) navigate(START_URL);"));
        assert!(html.contains("const START_URL = window.__CB_START_URL__ || null;"));
    }

    #[test]
    fn start_url_cannot_break_out_of_the_script_tag() {
        let html = index_html(Some("https://x/</script><script>alert(1)</script>"));
        assert!(
            !html.contains("</script><script>alert(1)"),
            "injection must be escaped: {html}"
        );
        assert!(html.contains("\\u003c/script"), "{html}");
    }

    #[tokio::test]
    async fn get_with_url_query_returns_page_carrying_that_url() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let store = cb_store::Store::open_in_memory().expect("store");
        let state = WebState::new(cb_core::Ctx::new(store, cb_net::Profile::Chrome));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router(state)).await;
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(
                b"GET /?url=https%3A%2F%2Fexample.com%2F HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("write");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.expect("read");
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("200"), "expected 200: {text}");
        assert!(
            text.contains("__CB_START_URL__ = \"https://example.com/\""),
            "response must carry the navigate target: {text}"
        );
    }

    #[test]
    fn span_text_flattens() {
        let spans = vec![
            Span {
                text: "a".into(),
                style: SpanStyle::Normal,
                link: None,
            },
            Span {
                text: "b".into(),
                style: SpanStyle::Bold,
                link: None,
            },
        ];
        assert_eq!(spans_text(&spans), "ab");
    }
}
