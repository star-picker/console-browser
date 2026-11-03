//! cb-net: HTTP client with browser TLS/HTTP2 fingerprint impersonation
//! (via `wreq` + `wreq_util` presets), plus a record/replay backend for offline tests.

pub mod cookies;

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    Chrome,
    Firefox,
    Safari,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Chrome => "chrome",
            Profile::Firefox => "firefox",
            Profile::Safari => "safari",
        }
    }

    pub fn parse(s: &str) -> Option<Profile> {
        match s {
            "chrome" => Some(Profile::Chrome),
            "firefox" => Some(Profile::Firefox),
            "safari" => Some(Profile::Safari),
            _ => None,
        }
    }

    fn emulation(self) -> wreq_util::Emulation {
        match self {
            Profile::Chrome => wreq_util::Emulation::Chrome137,
            Profile::Firefox => wreq_util::Emulation::Firefox139,
            Profile::Safari => wreq_util::Emulation::Safari18,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Request {
        Request {
            method: "GET".to_string(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn post(url: impl Into<String>, body: impl Into<String>) -> Request {
        Request {
            method: "POST".to_string(),
            url: url.into(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Some(body.into()),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Request {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn to_entry(&self, requested_url: &str) -> CassetteEntry {
        // cassette 会被提交/分享：凭证类响应头一律不落盘。
        let sensitive = [
            "set-cookie",
            "set-cookie2",
            "cookie",
            "authorization",
            "proxy-authorization",
            "x-api-key",
            "x-auth-token",
        ];
        CassetteEntry {
            url: requested_url.to_string(),
            final_url: self.final_url.clone(),
            status: self.status,
            headers: self
                .headers
                .iter()
                .filter(|(k, _)| !sensitive.contains(&k.to_ascii_lowercase().as_str()))
                .cloned()
                .collect(),
            body: String::from_utf8_lossy(&self.body).to_string(),
            error: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("url not recorded in cassette: {0}")]
    NotRecorded(String),
    #[error("network error: {0}")]
    Network(String),
    /// DNS 解析失败：域名不存在/解析不了，重试没有意义。
    #[error("{0}")]
    Dns(String),
    /// 请求超时（连接或读取）：可重试。
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("http error: status {0}")]
    Http(u16),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cassette error: {0}")]
    Cassette(String),
}

/// 底层错误链里是不是 DNS 解析失败。
///
/// 各家底层库措辞不同（hyper/rustls/getaddrinfo），按已知特征串判，宁可漏判成
/// network（可重试）也不要把真网络问题标成不可重试。
pub fn looks_like_dns_error(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "dns error",
        "dns resolution",
        "failed to lookup address",
        "name or service not known",
        "nodename nor servname",
        "no such host",
        "getaddrinfo",
        "failed to resolve",
    ]
    .iter()
    .any(|needle| m.contains(needle))
}

/// 把 wreq 错误本身与 source 链拼成一条可诊断的字符串。
fn error_chain_text(e: &wreq::Error) -> String {
    use std::error::Error as _;
    let mut out = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    if let Some(url) = e.url() {
        if !out.contains(url.as_str()) {
            out.push_str(" (url: ");
            out.push_str(url.as_str());
            out.push(')');
        }
    }
    out
}

impl From<wreq::Error> for NetError {
    fn from(e: wreq::Error) -> Self {
        let detail = error_chain_text(&e);
        if looks_like_dns_error(&detail) {
            let host = e.url().and_then(|u| u.host_str()).map(|h| h.to_string());
            return match host {
                Some(host) => NetError::Dns(format!("DNS lookup failed for {host}: {detail}")),
                None => NetError::Dns(format!("DNS lookup failed: {detail}")),
            };
        }
        if e.is_timeout() {
            return NetError::Timeout(detail);
        }
        NetError::Network(detail)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    pub url: String,
    pub final_url: String,
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: String,
    /// 回放时模拟失败：timeout / network / dns（测试重试与错误分类用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cassette {
    pub version: u32,
    pub entries: Vec<CassetteEntry>,
}

impl Default for Cassette {
    fn default() -> Cassette {
        Cassette::new()
    }
}

impl Cassette {
    pub fn new() -> Cassette {
        Cassette {
            version: 1,
            entries: Vec::new(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Cassette, NetError> {
        let text = std::fs::read_to_string(path.as_ref())?;
        serde_json::from_str(&text).map_err(|e| NetError::Cassette(e.to_string()))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), NetError> {
        if let Some(parent) = path.as_ref().parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| NetError::Cassette(e.to_string()))?;
        std::fs::write(path.as_ref(), text)?;
        Ok(())
    }
}

/// 三个阶段分开的超时：连不上不该把总预算耗光。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub connect: std::time::Duration,
    pub read: std::time::Duration,
    /// None = 不设客户端总超时（调用方自己套 outer timeout）。
    pub total: Option<std::time::Duration>,
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts {
            connect: std::time::Duration::from_secs(5),
            read: std::time::Duration::from_secs(25),
            total: None,
        }
    }
}

#[derive(Clone)]
enum Backend {
    Live {
        client: wreq::Client,
        profile: Profile,
    },
    Replay(Cassette),
}

/// 克隆是轻量的：底层 client/cassette 共享，回放游标也共享。
/// 一次 fetch 的多次尝试因此能消费同一条 cassette 里的序列。
#[derive(Clone)]
pub struct NetClient {
    backend: Backend,
    /// 可选 cookie jar：发请求时带上、收响应时落库。
    cookie_store: Option<std::sync::Arc<dyn cookies::CookieStore>>,
    /// 录制模式下累计的响应，供 `--record` 落盘。
    record: std::sync::Arc<std::sync::Mutex<Option<Cassette>>>,
    /// 回放游标：同一 URL 可以录多条（成功/失败/超时），按顺序消费。
    replay_cursor: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, usize>>>,
}

impl NetClient {
    pub fn new(profile: Profile) -> Result<NetClient, NetError> {
        NetClient::new_with_timeouts(profile, Timeouts::default())
    }

    pub fn new_with_timeouts(profile: Profile, timeouts: Timeouts) -> Result<NetClient, NetError> {
        let mut builder = wreq::Client::builder()
            .emulation(profile.emulation())
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read);
        if let Some(total) = timeouts.total {
            builder = builder.timeout(total);
        }
        let client = builder
            .build()
            .map_err(|e| NetError::Network(format!("build client: {e}")))?;
        Ok(NetClient {
            backend: Backend::Live { client, profile },
            cookie_store: None,
            record: std::sync::Arc::new(std::sync::Mutex::new(None)),
            replay_cursor: std::sync::Arc::new(std::sync::Mutex::default()),
        })
    }

    pub fn replay(path: impl AsRef<Path>) -> Result<NetClient, NetError> {
        Ok(NetClient {
            backend: Backend::Replay(Cassette::load(path)?),
            cookie_store: None,
            record: std::sync::Arc::new(std::sync::Mutex::new(None)),
            replay_cursor: std::sync::Arc::new(std::sync::Mutex::default()),
        })
    }

    /// 挂上 cookie jar（登录态复用）。返回 self，便于链式构造。
    pub fn with_cookie_store(
        mut self,
        store: std::sync::Arc<dyn cookies::CookieStore>,
    ) -> NetClient {
        self.cookie_store = Some(store);
        self
    }

    /// 目标 URL 是否已有看起来像会话的 cookie。
    pub fn has_session_cookie(&self, url: &str) -> bool {
        let Some(store) = &self.cookie_store else {
            return false;
        };
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        let now = now_secs();
        store.load(host).into_iter().any(|c| {
            !c.is_expired(now) && c.looks_like_session() && cookies::domain_matches(&c.domain, host)
        })
    }

    fn cookie_header_for(&self, url: &str) -> Option<String> {
        let store = self.cookie_store.as_ref()?;
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        let path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        let https = parsed.scheme() == "https";
        let cookies = store.load(&host);
        cookies::header_value(&cookies, &host, path, https, now_secs())
    }

    fn store_response_cookies(&self, url: &str, headers: &[(String, String)]) {
        let Some(store) = self.cookie_store.as_ref() else {
            return;
        };
        let Ok(parsed) = url::Url::parse(url) else {
            return;
        };
        let Some(host) = parsed.host_str() else {
            return;
        };
        let mut parsed_cookies = Vec::new();
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("set-cookie") {
                if let Some(cookie) = cookies::parse_set_cookie(v, host, now_secs()) {
                    parsed_cookies.push(cookie);
                }
            }
        }
        if !parsed_cookies.is_empty() {
            store.save(&parsed_cookies);
        }
    }

    pub fn profile(&self) -> Option<Profile> {
        match &self.backend {
            Backend::Live { profile, .. } => Some(*profile),
            Backend::Replay(_) => None,
        }
    }

    /// 打开录制：后续响应会写进内存 cassette。
    pub fn start_recording(&self) {
        *self.record.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cassette::new());
    }

    /// 落盘录制的 cassette。
    pub fn save_recording(&self, path: impl AsRef<Path>) -> Result<(), NetError> {
        let guard = self.record.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(c) => c.save(path),
            None => Err(NetError::Cassette("recording not started".to_string())),
        }
    }

    pub async fn get(&self, req: &Request) -> Result<Response, NetError> {
        match &self.backend {
            Backend::Replay(cassette) => {
                let index = {
                    let mut cursor = self.replay_cursor.lock().unwrap_or_else(|e| e.into_inner());
                    let slot = cursor.entry(req.url.clone()).or_insert(0);
                    let index = *slot;
                    *slot += 1;
                    index
                };
                let matches: Vec<&CassetteEntry> = cassette
                    .entries
                    .iter()
                    .filter(|e| e.url == req.url)
                    .collect();
                if matches.is_empty() {
                    return Err(NetError::NotRecorded(req.url.clone()));
                }
                // 条目用完后重复最后一条：同一响应回放 N 次（bench 会用到）。
                let entry = matches[index.min(matches.len() - 1)];
                if let Some(kind) = &entry.error {
                    return Err(match kind.as_str() {
                        "timeout" => NetError::Timeout(format!("replayed timeout for {}", req.url)),
                        "dns" => NetError::Dns(format!("DNS lookup failed for {}", req.url)),
                        other => NetError::Network(format!("replayed {other} for {}", req.url)),
                    });
                }
                Ok(Response {
                    status: entry.status,
                    headers: entry
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    body: entry.body.as_bytes().to_vec(),
                    final_url: entry.final_url.clone(),
                })
            }
            Backend::Live { client, .. } => {
                let mut builder = match req.method.to_ascii_uppercase().as_str() {
                    "GET" => client.get(&req.url),
                    "HEAD" => client.head(&req.url),
                    "POST" => client.post(&req.url),
                    other => return Err(NetError::Network(format!("unsupported method {other}"))),
                };
                if let Some(body) = &req.body {
                    builder = builder.body(body.clone());
                }
                for (k, v) in &req.headers {
                    builder = builder.header(k.as_str(), v.as_str());
                }
                let explicit_cookie = req
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("cookie"));
                if !explicit_cookie {
                    if let Some(header) = self.cookie_header_for(&req.url) {
                        builder = builder.header("cookie", header);
                    }
                }
                let resp = builder.send().await?;
                let status = resp.status().as_u16();
                let final_url = resp.url().as_str().to_string();
                let headers: Vec<(String, String)> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().to_string(),
                            v.to_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect();
                let body = resp.bytes().await?.to_vec();
                let out = Response {
                    status,
                    headers,
                    body,
                    final_url,
                };
                // 响应里的 Set-Cookie 落到 jar（会话复用就靠这一步）
                self.store_response_cookies(&req.url, &out.headers);
                if let Some(cassette) = self
                    .record
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_mut()
                {
                    cassette.entries.push(out.to_entry(&req.url));
                }
                Ok(out)
            }
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 归一化 URL：小写 scheme/host，去掉 fragment 与默认端口。
pub fn normalize_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.trim().to_string();
    };
    parsed.set_fragment(None);
    let scheme = parsed.scheme().to_ascii_lowercase();
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let port = match (scheme.as_str(), parsed.port()) {
        ("https", Some(443)) | ("http", Some(80)) => None,
        (_, p) => p,
    };
    let path = parsed.path().to_string();
    let query = parsed.query().map(|q| format!("?{q}")).unwrap_or_default();
    match port {
        Some(p) => format!("{scheme}://{host}:{p}{path}{query}"),
        None => format!("{scheme}://{host}{path}{query}"),
    }
}

/// 缓存键：方法 + 归一化 URL + 指纹 profile + 是否执行 JS。
pub fn cache_key(method: &str, url: &str, profile: Profile, js: bool) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(method.to_ascii_uppercase().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(normalize_url(url).as_bytes());
    hasher.update(b"\x1f");
    hasher.update(profile.as_str().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(if js { b"js1" } else { b"js0" });
    hasher.finalize().to_hex().to_string()
}
