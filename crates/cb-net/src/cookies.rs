//! Cookie jar：保存/发送 Cookie，供登录态复用。
//!
//! 设计：cb-net 只定义 `Cookie` 与 `CookieStore` 接口，实际存储由上层（cb-store + 加密）实现，
//! 避免 cb-net 反向依赖 sqlite。

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub domain: String,
    pub name: String,
    pub path: String,
    pub value: String,
    /// Unix 秒；None = 会话 cookie。
    pub expires_at: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    /// 没有 Domain 属性时为 host-only，只对该主机生效。
    pub host_only: bool,
}

impl Cookie {
    pub fn is_expired(&self, now: i64) -> bool {
        matches!(self.expires_at, Some(ts) if ts <= now)
    }

    /// 名字看起来像会话凭证（给「是否已登录」判断用，不做安全承诺）。
    pub fn looks_like_session(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        ["session", "sid", "token", "auth", "jwt", "login", "sso"]
            .iter()
            .any(|hint| name.contains(hint))
    }
}

pub trait CookieStore: Send + Sync {
    fn load(&self, domain: &str) -> Vec<Cookie>;
    fn save(&self, cookies: &[Cookie]);
    fn clear(&self, domain: Option<&str>);
}

/// 进程内存实现：测试与一次性会话用。
#[derive(Default)]
pub struct MemoryCookieStore {
    inner: Mutex<Vec<Cookie>>,
}

impl MemoryCookieStore {
    pub fn new() -> MemoryCookieStore {
        MemoryCookieStore {
            inner: Mutex::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<Cookie> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl CookieStore for MemoryCookieStore {
    fn load(&self, domain: &str) -> Vec<Cookie> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|c| domain_matches(&c.domain, domain))
            .cloned()
            .collect()
    }

    fn save(&self, cookies: &[Cookie]) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for cookie in cookies {
            guard.retain(|c| {
                !(c.domain == cookie.domain && c.name == cookie.name && c.path == cookie.path)
            });
            guard.push(cookie.clone());
        }
    }

    fn clear(&self, domain: Option<&str>) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match domain {
            Some(d) => guard.retain(|c| !domain_matches(&c.domain, d)),
            None => guard.clear(),
        }
    }
}

/// 域名匹配：cookie 域后缀匹配请求主机（".example.com" 也覆盖子域）。
pub fn domain_matches(cookie_domain: &str, host: &str) -> bool {
    let cookie_domain = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    if cookie_domain.is_empty() {
        return false;
    }
    host == cookie_domain || host.ends_with(&format!(".{cookie_domain}"))
}

pub fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    let cookie_path = if cookie_path.is_empty() {
        "/"
    } else {
        cookie_path
    };
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/')
                || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')))
}

/// 拼出 `Cookie:` 头；已过期的跳过；secure cookie 只走 https。
pub fn header_value(
    cookies: &[Cookie],
    host: &str,
    path: &str,
    https: bool,
    now: i64,
) -> Option<String> {
    let mut pairs: Vec<String> = Vec::new();
    for cookie in cookies {
        if cookie.is_expired(now) {
            continue;
        }
        if cookie.secure && !https {
            continue;
        }
        if !domain_matches(&cookie.domain, host) || !path_matches(&cookie.path, path) {
            continue;
        }
        pairs.push(format!("{}={}", cookie.name, cookie.value));
    }
    if pairs.is_empty() {
        None
    } else {
        Some(pairs.join("; "))
    }
}

/// 解析一条 Set-Cookie。拿不准的属性就保守丢弃（比如复杂的 Expires 日期）。
pub fn parse_set_cookie(raw: &str, request_host: &str, now: i64) -> Option<Cookie> {
    let mut parts = raw.split(';');
    let first = parts.next()?.trim();
    let (name, value) = first.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut cookie = Cookie {
        domain: request_host.to_ascii_lowercase(),
        name: name.to_string(),
        path: "/".to_string(),
        value: value.trim().to_string(),
        expires_at: None,
        secure: false,
        http_only: false,
        same_site: None,
        host_only: true,
    };
    for attr in parts {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim().to_string()),
            None => (attr.to_ascii_lowercase(), String::new()),
        };
        match key.as_str() {
            "domain" => {
                let d = val.trim_start_matches('.').to_ascii_lowercase();
                if !d.is_empty() && domain_matches(&d, request_host) {
                    cookie.domain = d;
                    cookie.host_only = false;
                }
            }
            "path" => {
                if val.starts_with('/') {
                    cookie.path = val;
                }
            }
            "max-age" => {
                if let Ok(secs) = val.parse::<i64>() {
                    cookie.expires_at = Some(now + secs.max(0));
                }
            }
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            "samesite" => cookie.same_site = Some(val),
            _ => {}
        }
    }
    Some(cookie)
}

pub fn shared(store: MemoryCookieStore) -> Arc<dyn CookieStore> {
    Arc::new(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(domain: &str, name: &str, value: &str) -> Cookie {
        Cookie {
            domain: domain.to_string(),
            name: name.to_string(),
            path: "/".to_string(),
            value: value.to_string(),
            expires_at: None,
            secure: false,
            http_only: false,
            same_site: None,
            host_only: true,
        }
    }

    #[test]
    fn domain_suffix_matching() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(domain_matches(".example.com", "a.example.com"));
        assert!(!domain_matches("example.com", "notexample.com"));
        assert!(!domain_matches("", "example.com"));
    }

    #[test]
    fn path_matching() {
        assert!(path_matches("/", "/anything"));
        assert!(path_matches("/a", "/a/b"));
        assert!(!path_matches("/a", "/ab"));
    }

    #[test]
    fn parses_set_cookie_attributes() {
        let c = parse_set_cookie("sid=abc123; Path=/app; Domain=.example.com; Max-Age=60; Secure; HttpOnly; SameSite=Lax", "app.example.com", 1000).expect("cookie");
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.domain, "example.com");
        assert_eq!(c.path, "/app");
        assert_eq!(c.expires_at, Some(1060));
        assert!(c.secure && c.http_only);
        assert_eq!(c.same_site.as_deref(), Some("Lax"));
        assert!(!c.host_only);
    }

    #[test]
    fn header_skips_expired_and_insecure() {
        let mut expired = cookie("example.com", "old", "1");
        expired.expires_at = Some(500);
        let mut secure = cookie("example.com", "s", "2");
        secure.secure = true;
        let jar = vec![expired, secure, cookie("example.com", "plain", "3")];
        let header = header_value(&jar, "example.com", "/", false, 1000).expect("header");
        assert_eq!(header, "plain=3");
        let https_header = header_value(&jar, "example.com", "/", true, 1000).expect("header");
        assert!(https_header.contains("s=2"));
        assert!(!https_header.contains("old=1"));
    }

    #[test]
    fn memory_store_roundtrip_and_clear() {
        let store = MemoryCookieStore::new();
        store.save(&[cookie("example.com", "sid", "1")]);
        assert_eq!(store.load("example.com").len(), 1);
        assert_eq!(store.load("other.com").len(), 0);
        store.save(&[cookie("example.com", "sid", "2")]);
        assert_eq!(store.load("example.com")[0].value, "2");
        store.clear(Some("example.com"));
        assert!(store.load("example.com").is_empty());
    }

    #[test]
    fn session_cookie_heuristic() {
        assert!(cookie("x", "sessionid", "1").looks_like_session());
        assert!(cookie("x", "auth_token", "1").looks_like_session());
        assert!(!cookie("x", "theme", "dark").looks_like_session());
    }
}
