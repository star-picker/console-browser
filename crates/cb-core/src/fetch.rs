//! 取页面主流程：缓存 → 网络 → DOM → 提取 → 存库 → 历史。

use crate::error::CbError;
use crate::Ctx;
use cb_extract::{ExtractOpts, Markdown};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_CACHE_TTL_SECS: i64 = 7 * 86_400;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Md,
    Html,
    Text,
    Json,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s {
            "md" | "markdown" => Some(Format::Md),
            "html" => Some(Format::Html),
            "text" | "txt" => Some(Format::Text),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchOpts {
    pub url: String,
    pub format: Format,
    pub max_tokens: Option<usize>,
    pub scope: Option<String>,
    pub fresh: bool,
    pub no_js: bool,
    pub replay: Option<PathBuf>,
    pub record: Option<PathBuf>,
    pub timeout_ms: u64,
}

impl Default for FetchOpts {
    fn default() -> Self {
        FetchOpts {
            url: String::new(),
            format: Format::Md,
            max_tokens: None,
            scope: None,
            fresh: false,
            no_js: false,
            replay: None,
            record: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchOutcome {
    pub url: String,
    pub final_url: String,
    pub status: u16,
    pub content_type: String,
    pub title: String,
    pub markdown: String,
    pub raw: String,
    pub tokens_estimate: usize,
    pub links: Vec<cb_extract::Link>,
    pub images: Vec<cb_extract::Image>,
    pub truncated: bool,
    pub cached: bool,
    pub cache_age_s: Option<i64>,
    pub bytes: usize,
}

/// 解析 Cache-Control: max-age=N，失败返回默认 TTL。
pub fn cache_ttl_secs(headers: &[(String, String)]) -> i64 {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("cache-control") {
            for part in v.split(',') {
                let part = part.trim();
                if let Some(rest) = part.strip_prefix("max-age=") {
                    if let Ok(n) = rest.trim().parse::<i64>() {
                        return n.max(0);
                    }
                }
                if part.eq_ignore_ascii_case("no-store") || part.eq_ignore_ascii_case("no-cache") {
                    return 0;
                }
            }
        }
    }
    DEFAULT_CACHE_TTL_SECS
}

fn status_error(status: u16, url: &str) -> CbError {
    match status {
        401 | 407 => CbError::needs_auth(format!("http {status}"), url),
        403 | 429 | 503 => CbError::challenge(
            format!("http {status}: looks like an anti-bot response"),
            url,
            "escalate: rotate profile, warm cookies, or hand off to the calling agent",
        ),
        404 | 410 => CbError::not_found(format!("http {status}")),
        s if (500..600).contains(&s) => CbError::network(format!("http {s}")),
        s => CbError::network(format!("http {s}")),
    }
}

pub async fn fetch_page(ctx: &Ctx, opts: &FetchOpts) -> Result<FetchOutcome, CbError> {
    if opts.url.trim().is_empty() {
        return Err(CbError::usage("url is required"));
    }
    let now = ctx.now();
    let key = cb_net::cache_key("GET", &opts.url, ctx.profile, !opts.no_js);

    if !opts.fresh {
        if let Some(entry) = ctx.store.cache_get(&key).map_err(crate::error::store_err)? {
            if entry.expires_at > now {
                let html = String::from_utf8_lossy(&entry.body).to_string();
                let age = now - entry.created_at;
                let headers: Vec<(String, String)> = serde_json::from_str::<
                    std::collections::BTreeMap<String, String>,
                >(&entry.headers)
                .map(|m| m.into_iter().collect())
                .unwrap_or_default();
                return build_outcome(
                    ctx,
                    opts,
                    &opts.url,
                    &entry.url,
                    entry.status as u16,
                    &headers,
                    html,
                    true,
                    Some(age),
                );
            }
        }
    }

    let client = match &opts.replay {
        Some(path) => cb_net::NetClient::replay(path)?,
        None => cb_net::NetClient::new(ctx.profile)?,
    };
    if let Some(path) = &opts.record {
        client.start_recording();
        tracing::debug!(path = %path.display(), "recording enabled");
    }

    let request = cb_net::Request::get(opts.url.clone());
    let resp = match tokio::time::timeout(
        std::time::Duration::from_millis(opts.timeout_ms),
        client.get(&request),
    )
    .await
    {
        Err(_) => {
            return Err(CbError::timeout(format!(
                "no response within {}ms",
                opts.timeout_ms
            )))
        }
        Ok(Err(e)) => return Err(e.into()),
        Ok(Ok(r)) => r,
    };

    if let Some(path) = &opts.record {
        client.save_recording(path)?;
    }

    let content_type = resp.header("content-type").unwrap_or("").to_string();
    let body_text = resp.text();

    if resp.status >= 400 {
        return Err(status_error(resp.status, &opts.url));
    }

    let outcome = build_outcome(
        ctx,
        opts,
        &opts.url,
        &resp.final_url,
        resp.status,
        &resp.headers,
        body_text.clone(),
        false,
        None,
    )?;

    let ttl = cache_ttl_secs(&resp.headers);
    if ttl > 0 {
        let headers_json = serde_json::to_string(
            &resp
                .headers
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<String, String>>(),
        )
        .unwrap_or_else(|_| "{}".to_string());
        let validators = serde_json::json!({
            "etag": resp.header("etag"),
            "last_modified": resp.header("last-modified"),
        })
        .to_string();
        ctx.store.cache_put(&cb_store::CacheEntry {
            key,
            method: "GET".to_string(),
            url: opts.url.clone(),
            status: resp.status as i64,
            headers: headers_json,
            body: resp.body.clone(),
            expires_at: now + ttl,
            validators,
            created_at: now,
        })?;
    }

    let _ = content_type;
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn build_outcome(
    ctx: &Ctx,
    opts: &FetchOpts,
    requested_url: &str,
    final_url: &str,
    status: u16,
    headers: &[(String, String)],
    html: String,
    cached: bool,
    cache_age_s: Option<i64>,
) -> Result<FetchOutcome, CbError> {
    let doc = cb_dom::Document::parse(&html);
    let title = doc.title().unwrap_or_default();
    let extract_opts = ExtractOpts {
        scope: opts.scope.clone(),
        max_tokens: opts.max_tokens,
        ..Default::default()
    };
    let md: Markdown = cb_extract::extract_markdown(&html, &extract_opts)
        .map_err(|e| CbError::other(format!("extract: {e}")))?;

    let content_type = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "text/html".to_string());

    let now = ctx.now();
    let content_sha = blake3::hash(html.as_bytes()).to_hex().to_string();
    ctx.store.put_page(&cb_store::PageRow {
        id: 0,
        url: requested_url.to_string(),
        url_hash: cb_store::hash_url(requested_url),
        canonical: cb_net::normalize_url(final_url),
        site: url::Url::parse(final_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default(),
        title: title.clone(),
        status: status as i64,
        content_type: content_type.clone(),
        fetched_at: now,
        content_sha,
        md: md.text.as_bytes().to_vec(),
        raw: Some(html.as_bytes().to_vec()),
        etag: headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("etag"))
            .map(|(_, v)| v.clone()),
        last_modified: headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("last-modified"))
            .map(|(_, v)| v.clone()),
        bytes: html.len() as i64,
        tokens: md.tokens_estimate as i64,
        lang: None,
    })?;

    ctx.store.append_history(&cb_store::HistoryRow {
        id: 0,
        ts: now,
        url: requested_url.to_string(),
        title: title.clone(),
        kind: "fetch".to_string(),
        session_id: None,
        duration_ms: 0,
        ok: true,
    })?;

    Ok(FetchOutcome {
        url: requested_url.to_string(),
        final_url: final_url.to_string(),
        status,
        content_type,
        title,
        markdown: md.text,
        raw: html,
        tokens_estimate: md.tokens_estimate,
        links: md.links,
        images: md.images,
        truncated: md.truncated,
        cached,
        cache_age_s,
        bytes: 0,
    })
}
