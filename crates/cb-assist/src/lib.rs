//! cb-assist：人工接力（登录 / 人机验证）。
//!
//! 思路：cb 自己不嵌浏览器内核。要人动手时，借系统里已有的 Edge/Chrome：
//! 用临时 profile 起一个窗口打开目标页，人登录/过滑块；cb 通过 CDP 把 cookie 收回来，
//! 写进本地 jar 后自动重载。没有 Chromium 浏览器时降级为粘贴 Cookie。

use cb_net::cookies::Cookie;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserKind {
    Edge,
    Chrome,
}

impl BrowserKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BrowserKind::Edge => "edge",
            BrowserKind::Chrome => "chrome",
        }
    }

    pub fn parse(raw: &str) -> Option<BrowserKind> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "edge" | "msedge" => Some(BrowserKind::Edge),
            "chrome" | "google-chrome" => Some(BrowserKind::Chrome),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssistOpts {
    pub url: String,
    pub browser: Option<BrowserKind>,
    pub keep_open: bool,
    pub timeout: Duration,
    pub poll: Duration,
    /// 测试用：跳过启动浏览器，直接连这个调试端口。
    pub attach_port: Option<u16>,
}

impl AssistOpts {
    pub fn new(url: impl Into<String>) -> AssistOpts {
        AssistOpts {
            url: url.into(),
            browser: None,
            keep_open: false,
            timeout: Duration::from_secs(300),
            poll: Duration::from_millis(800),
            attach_port: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AssistEvent {
    Started { browser: BrowserKind, port: u16 },
    CookiesUpdated { count: usize },
    Finished { cookies: Vec<Cookie> },
    Failed { message: String },
}

/// 常见安装位置：先按 Windows 路径找。
pub fn find_browser(prefer: Option<BrowserKind>) -> Option<(BrowserKind, PathBuf)> {
    let candidates: &[(BrowserKind, &[&str])] = &[
        (
            BrowserKind::Edge,
            &[
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            ],
        ),
        (
            BrowserKind::Chrome,
            &[
                r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            ],
        ),
    ];

    let mut ordered: Vec<(BrowserKind, &[&str])> = candidates.to_vec();
    if let Some(preferred) = prefer {
        ordered.sort_by_key(|(kind, _)| if *kind == preferred { 0 } else { 1 });
    }
    for (kind, paths) in ordered {
        for path in paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some((kind, p));
            }
        }
    }
    None
}

/// 从 Cookie 头或 a=1; b=2 这种粘贴文本里解析 cookie（域取自目标 URL）。
pub fn import_manual_cookies(raw: &str, url: &str) -> Vec<Cookie> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default();
    if host.is_empty() {
        return Vec::new();
    }
    let cleaned = raw.trim();
    let cleaned = cleaned
        .strip_prefix("Cookie:")
        .or_else(|| cleaned.strip_prefix("cookie:"))
        .unwrap_or(cleaned)
        .trim();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let lowered_all = cleaned.to_ascii_lowercase();
    // 整串像一条 Set-Cookie（带属性）时，整体解析，别按分号拆散属性。
    let looks_like_set_cookie = lowered_all.contains("path=")
        || lowered_all.contains("domain=")
        || lowered_all.contains("httponly")
        || lowered_all.contains("samesite=");
    if looks_like_set_cookie {
        return cb_net::cookies::parse_set_cookie(cleaned, &host, now)
            .into_iter()
            .collect();
    }
    let mut out = Vec::new();
    for part in cleaned.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let lowered = part.to_ascii_lowercase();
        let parsed = if lowered.contains("path=") || lowered.contains("domain=") {
            cb_net::cookies::parse_set_cookie(part, &host, now)
        } else {
            let (name, value) = match part.split_once('=') {
                Some((n, v)) => (n.trim(), v.trim()),
                None => continue,
            };
            if name.is_empty() {
                continue;
            }
            Some(Cookie {
                domain: host.clone(),
                name: name.to_string(),
                path: "/".to_string(),
                value: value.to_string(),
                expires_at: None,
                secure: false,
                http_only: false,
                same_site: None,
                host_only: true,
            })
        };
        if let Some(cookie) = parsed {
            out.push(cookie);
        }
    }
    out
}

/// 从 /json/list 里挑出目标标签页的 WebSocket 调试地址。
pub fn pick_target(body: &str, want_url: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let targets = value.as_array()?;
    let mut fallback: Option<String> = None;
    for target in targets {
        let ws = target["webSocketDebuggerUrl"]
            .as_str()
            .map(|s| s.to_string());
        let url = target["url"].as_str().unwrap_or_default();
        if ws.is_none() {
            continue;
        }
        if !url.starts_with("devtools://") {
            if fallback.is_none() {
                fallback = ws.clone();
            }
            if url.starts_with(want_url) || want_url.starts_with(url) {
                return ws;
            }
        }
    }
    fallback
}

/// 解析一条 CDP 响应里的 cookie 列表（Network.getAllCookies）。
pub fn cookies_from_cdp(message: &str) -> Option<Vec<Cookie>> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    let cookies = value["result"]["cookies"].as_array()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut out = Vec::new();
    for c in cookies {
        let name = match c["name"].as_str() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let domain = c["domain"].as_str().unwrap_or_default().to_string();
        if domain.is_empty() {
            continue;
        }
        let expires = c["expires"].as_f64().unwrap_or(-1.0);
        let expires_at = if expires > 0.0 {
            Some(expires as i64)
        } else {
            None
        };
        if matches!(expires_at, Some(ts) if ts <= now) {
            continue;
        }
        out.push(Cookie {
            domain,
            name,
            path: c["path"].as_str().unwrap_or("/").to_string(),
            value: c["value"].as_str().unwrap_or_default().to_string(),
            expires_at,
            secure: c["secure"].as_bool().unwrap_or(false),
            http_only: c["httpOnly"].as_bool().unwrap_or(false),
            same_site: c["sameSite"].as_str().map(|s| s.to_string()),
            host_only: false,
        });
    }
    Some(out)
}

pub fn has_session_cookie(cookies: &[Cookie]) -> bool {
    cookies.iter().any(|c| c.looks_like_session())
}

// ---------------- 真正的接力流程（异步） ----------------

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

fn pick_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(9222)
}

async fn http_get(url: &str) -> anyhow::Result<String> {
    let client = cb_net::NetClient::new(cb_net::Profile::Chrome)?;
    let resp = client.get(&cb_net::Request::get(url)).await?;
    if resp.status >= 400 {
        anyhow::bail!("{url} -> http {}", resp.status);
    }
    Ok(resp.text())
}

async fn wait_for_devtools(base: &str, timeout: Duration) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    let version_url = format!("{base}/json/version");
    loop {
        if http_get(&version_url).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("browser devtools endpoint did not come up at {base}");
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// 起一个临时浏览器，等人完成登录/验证，然后回收 cookie。
/// 没有 Chromium 浏览器时返回错误（调用方应提示改用粘贴 Cookie）。
pub async fn run_assist<F>(opts: &AssistOpts, mut on_event: F) -> anyhow::Result<Vec<Cookie>>
where
    F: FnMut(AssistEvent),
{
    let mut child: Option<std::process::Child> = None;
    let mut temp_dir: Option<PathBuf> = None;
    let (browser, port) = match opts.attach_port {
        Some(port) => (opts.browser.unwrap_or(BrowserKind::Edge), port),
        None => {
            let (browser, exe) = match find_browser(opts.browser) {
                Some(found) => found,
                None => {
                    on_event(AssistEvent::Failed {
                        message: "no Chromium browser (Edge/Chrome) found".to_string(),
                    });
                    anyhow::bail!(
                        "no Chromium browser (Edge/Chrome) found; use --manual to paste cookies"
                    );
                }
            };
            let port = pick_port();
            let dir = std::env::temp_dir().join(format!("cb-assist-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let spawned = std::process::Command::new(&exe)
                .arg(format!("--user-data-dir={}", dir.display()))
                .arg(format!("--remote-debugging-port={port}"))
                .args([
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--new-window",
                    &opts.url,
                ])
                .spawn();
            match spawned {
                Ok(child_handle) => {
                    child = Some(child_handle);
                    temp_dir = Some(dir);
                }
                Err(e) => {
                    on_event(AssistEvent::Failed {
                        message: format!("failed to launch browser: {e}"),
                    });
                    anyhow::bail!("failed to launch browser: {e}");
                }
            }
            (browser, port)
        }
    };

    on_event(AssistEvent::Started { browser, port });
    let base = format!("http://127.0.0.1:{port}");

    let result = async {
        wait_for_devtools(&base, Duration::from_secs(30)).await?;
        let list = http_get(&format!("{base}/json/list")).await?;
        let ws_url = pick_target(&list, &opts.url)
            .ok_or_else(|| anyhow::anyhow!("no debuggable page target found"))?;
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await?;
        // 打开 Network 域，之后才能读 cookie
        let _ = ws
            .send(Message::Text(
                "{\"id\":1,\"method\":\"Network.enable\"}".into(),
            ))
            .await;

        let deadline = tokio::time::Instant::now() + opts.timeout;
        let mut next_id = 2u64;
        let mut latest: Vec<Cookie> = Vec::new();
        loop {
            let request = format!("{{\"id\":{next_id},\"method\":\"Network.getAllCookies\"}}");
            let _ = ws.send(Message::Text(request.into())).await;
            let wait = tokio::time::timeout(opts.poll, ws.next()).await;
            if let Ok(Some(Ok(Message::Text(text)))) = wait {
                {
                    if let Some(cookies) = cookies_from_cdp(&text) {
                        if cookies.len() != latest.len() {
                            on_event(AssistEvent::CookiesUpdated {
                                count: cookies.len(),
                            });
                        }
                        latest = cookies;
                        if has_session_cookie(&latest) {
                            break;
                        }
                    }
                }
            }
            next_id += 1;
            if tokio::time::Instant::now() >= deadline {
                break;
            }
        }
        let _ = ws.close(None).await;
        Ok::<Vec<Cookie>, anyhow::Error>(latest)
    }
    .await;

    if let Some(mut child) = child {
        if !opts.keep_open {
            let _ = child.kill();
        }
    }
    if let Some(dir) = temp_dir {
        if !opts.keep_open {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    match result {
        Ok(cookies) => {
            on_event(AssistEvent::Finished {
                cookies: cookies.clone(),
            });
            Ok(cookies)
        }
        Err(e) => {
            on_event(AssistEvent::Failed {
                message: e.to_string(),
            });
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_paste_parses_cookie_header() {
        let cookies = import_manual_cookies("Cookie: sid=abc; theme=dark", "https://a.example/x");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "sid");
        assert_eq!(cookies[0].domain, "a.example");
        assert_eq!(cookies[1].value, "dark");
    }

    #[test]
    fn manual_paste_parses_set_cookie_lines() {
        let cookies =
            import_manual_cookies("sid=abc; Path=/app; Secure; HttpOnly", "https://a.example/");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].path, "/app");
        assert!(cookies[0].secure && cookies[0].http_only);
    }

    #[test]
    fn manual_paste_rejects_empty_url() {
        assert!(import_manual_cookies("a=b", "not a url").is_empty());
    }

    #[test]
    fn picks_matching_target_websocket() {
        let body = r#"[
          {"url":"devtools://x","webSocketDebuggerUrl":"ws://dev/1"},
          {"url":"https://a.example/login","webSocketDebuggerUrl":"ws://dev/2"}
        ]"#;
        assert_eq!(
            pick_target(body, "https://a.example/login").as_deref(),
            Some("ws://dev/2")
        );
        assert_eq!(
            pick_target(body, "https://other.example/").as_deref(),
            Some("ws://dev/2")
        );
        assert!(pick_target("not json", "https://a/").is_none());
    }

    #[test]
    fn parses_cdp_cookies_and_drops_expired() {
        let msg = r#"{"id":3,"result":{"cookies":[
          {"name":"sid","value":"v","domain":".example.com","path":"/","expires":-1,"secure":true,"httpOnly":true,"sameSite":"Lax"},
          {"name":"old","value":"x","domain":".example.com","path":"/","expires":1000}
        ]}}"#;
        let cookies = cookies_from_cdp(msg).expect("cookies");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "sid");
        assert!(cookies[0].secure && cookies[0].http_only);
        assert!(has_session_cookie(&cookies));
    }

    #[test]
    fn browser_kind_parsing() {
        assert_eq!(BrowserKind::parse("Edge"), Some(BrowserKind::Edge));
        assert_eq!(BrowserKind::parse("chrome"), Some(BrowserKind::Chrome));
        assert!(BrowserKind::parse("firefox").is_none());
    }
}
