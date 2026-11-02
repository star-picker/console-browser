//! sqlite 后端的 cookie jar：实现 cb-net 的 CookieStore 接口。

use crate::secrets::SecretBox;
use crate::{Store, StoredCookie};
use cb_net::cookies::{domain_matches, Cookie, CookieStore};
use std::sync::Arc;

pub struct StoreCookieJar {
    store: Store,
    secret: Arc<dyn SecretBox>,
}

impl StoreCookieJar {
    pub fn new(store: Store, secret: Arc<dyn SecretBox>) -> StoreCookieJar {
        StoreCookieJar { store, secret }
    }

    pub fn secret_name(&self) -> &'static str {
        self.secret.name()
    }

    fn decrypt(&self, row: StoredCookie) -> Cookie {
        Cookie {
            domain: row.domain,
            name: row.name,
            path: row.path,
            value: String::from_utf8_lossy(&self.secret.open(&row.value)).to_string(),
            expires_at: row.expires_at,
            secure: row.secure,
            http_only: row.http_only,
            same_site: row.same_site,
            host_only: row.host_only,
        }
    }
}

impl CookieStore for StoreCookieJar {
    fn load(&self, domain: &str) -> Vec<Cookie> {
        self.store
            .load_cookie_rows(domain)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| self.decrypt(row))
                    .filter(|c| domain_matches(&c.domain, domain))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn save(&self, cookies: &[Cookie]) {
        let rows: Vec<StoredCookie> = cookies
            .iter()
            .map(|c| StoredCookie {
                domain: c.domain.clone(),
                name: c.name.clone(),
                path: c.path.clone(),
                value: self.secret.seal(c.value.as_bytes()),
                expires_at: c.expires_at,
                secure: c.secure,
                http_only: c.http_only,
                same_site: c.same_site.clone(),
                host_only: c.host_only,
            })
            .collect();
        let _ = self.store.save_cookie_rows(&rows);
    }

    fn clear(&self, domain: Option<&str>) {
        let _ = self.store.clear_cookie_rows(domain);
    }
}

impl Clone for StoreCookieJar {
    fn clone(&self) -> Self {
        StoreCookieJar {
            store: self.store.clone(),
            secret: self.secret.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::PlainBox;

    fn jar() -> StoreCookieJar {
        StoreCookieJar::new(Store::open_in_memory().expect("store"), Arc::new(PlainBox))
    }

    fn cookie(name: &str, value: &str) -> Cookie {
        Cookie {
            domain: "example.com".to_string(),
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
    fn roundtrip_goes_through_secret_box() {
        let jar = jar();
        jar.save(&[cookie("sid", "super-secret")]);
        let raw = jar.store.load_cookie_rows("example.com").expect("rows");
        assert_eq!(raw.len(), 1);
        // PlainBox 只是加标记（明文存本机）；换 DPAPI 后同一测试改为断言不可读。
        let loaded = jar.load("example.com");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].value, "super-secret");
    }

    #[test]
    fn domain_isolation_and_clear() {
        let jar = jar();
        jar.save(&[cookie("sid", "1")]);
        assert_eq!(jar.load("other.com").len(), 0);
        assert_eq!(jar.load("sub.example.com").len(), 1);
        jar.clear(Some("example.com"));
        assert!(jar.load("example.com").is_empty());
    }
}
