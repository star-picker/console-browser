//! cb-net: HTTP client with browser TLS/HTTP2 fingerprint impersonation
//! (via `wreq` + `wreq_util` presets), plus a record/replay backend for offline tests.

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
}

impl Request {
    pub fn get(url: impl Into<String>) -> Request {
        Request {
            method: "GET".to_string(),
            url: url.into(),
            headers: Vec::new(),
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
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("url not recorded in cassette: {0}")]
    NotRecorded(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("http error: status {0}")]
    Http(u16),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cassette error: {0}")]
    Cassette(String),
}

impl From<wreq::Error> for NetError {
    fn from(e: wreq::Error) -> Self {
        NetError::Network(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    pub url: String,
    pub final_url: String,
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: String,
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

enum Backend {
    Live {
        client: wreq::Client,
        profile: Profile,
    },
    Replay(Cassette),
}

pub struct NetClient {
    backend: Backend,
    /// 录制模式下累计的响应，供 `--record` 落盘。
    record: std::sync::Mutex<Option<Cassette>>,
}

impl NetClient {
    pub fn new(profile: Profile) -> Result<NetClient, NetError> {
        let client = wreq::Client::builder()
            .emulation(profile.emulation())
            .build()
            .map_err(|e| NetError::Network(format!("build client: {e}")))?;
        Ok(NetClient {
            backend: Backend::Live { client, profile },
            record: std::sync::Mutex::new(None),
        })
    }

    pub fn replay(path: impl AsRef<Path>) -> Result<NetClient, NetError> {
        Ok(NetClient {
            backend: Backend::Replay(Cassette::load(path)?),
            record: std::sync::Mutex::new(None),
        })
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
                let entry = cassette
                    .entries
                    .iter()
                    .find(|e| e.url == req.url)
                    .ok_or_else(|| NetError::NotRecorded(req.url.clone()))?;
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
                    other => return Err(NetError::Network(format!("unsupported method {other}"))),
                };
                for (k, v) in &req.headers {
                    builder = builder.header(k.as_str(), v.as_str());
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
