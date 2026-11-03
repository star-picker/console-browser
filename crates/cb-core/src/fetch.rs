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
    /// 挑战升到最后一阶时，允许花 token 问模型（默认关闭）。
    pub ai_escalate: bool,
    /// markdown 里是否内联图片（默认开；--no-images 关）。
    pub images: bool,
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
            ai_escalate: false,
            images: true,
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
    /// 结构化文档（TUI/webui 用）；不进 JSON，避免体积爆炸。
    #[serde(skip)]
    pub doc: Option<cb_extract::RenderDoc>,
    /// 识别到的登录墙 / 人机验证（不自动解决，只报告）。
    pub login: Option<crate::login::LoginWall>,
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
        403 => CbError::challenge(
            format!("http {status}: looks like an anti-bot response"),
            url,
            "escalate: rotate profile, warm cookies, or hand off to the calling agent",
        ),
        404 | 410 => CbError::not_found(format!("http {status}")),
        // 429 / 5xx 是瞬时的，不是反爬：可重试，交给调用方决定。
        s => CbError::network(format!("http {s}")),
    }
}

pub async fn fetch_page(ctx: &Ctx, opts: &FetchOpts) -> Result<FetchOutcome, CbError> {
    if opts.url.trim().is_empty() {
        return Err(CbError::usage("url is required"));
    }
    let now = ctx.now();
    let key = cb_net::cache_key("GET", &opts.url, ctx.profile, !opts.no_js);
    let probe_host = url::Url::parse(&opts.url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default();
    let jar_has_session = ctx
        .cookies
        .load(&probe_host)
        .into_iter()
        .any(|c| !c.is_expired(crate::now_secs()) && c.looks_like_session());

    if !opts.fresh {
        if let Some(entry) = ctx.store.cache_get(&key).map_err(crate::error::store_err)? {
            if entry.expires_at > now {
                let age = now - entry.created_at;
                let headers: Vec<(String, String)> = serde_json::from_str::<
                    std::collections::BTreeMap<String, String>,
                >(&entry.headers)
                .map(|m| m.into_iter().collect())
                .unwrap_or_default();
                let html = decode_body(&entry.body, &headers);
                // 重定向后的真实地址要随缓存一起存，否则命中缓存时元数据会翻转。
                let final_url = serde_json::from_str::<serde_json::Value>(&entry.validators)
                    .ok()
                    .and_then(|v| v["final_url"].as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| entry.url.clone());
                return build_outcome(
                    ctx,
                    opts,
                    &opts.url,
                    &final_url,
                    entry.status as u16,
                    &headers,
                    html,
                    true,
                    Some(age),
                    jar_has_session,
                );
            }
        }
    }

    let domain = url::Url::parse(&opts.url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    // 反爬阶梯：403 就换一个浏览器指纹再试，最多三轮，全程留痕。
    let mut resp = None;
    let mut last_challenge: Option<CbError> = None;
    let mut last_challenge_body: Option<String> = None;
    for (attempt, profile) in challenge_ladder(ctx.profile).into_iter().enumerate() {
        let client = match &opts.replay {
            Some(path) => cb_net::NetClient::replay(path)?,
            None => cb_net::NetClient::new(profile)?.with_cookie_store(ctx.cookies.clone()),
        };
        if attempt == 0 {
            if let Some(path) = &opts.record {
                client.start_recording();
                tracing::debug!(path = %path.display(), "recording enabled");
            }
        }
        let request = cb_net::Request::get(opts.url.clone());
        let attempt_resp = match tokio::time::timeout(
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

        let strategy = format!("rotate-profile:{}", profile.as_str());
        if attempt_resp.status == 403 {
            let _ = ctx.store.record_challenge(&cb_store::ChallengeEvent {
                ts: ctx.now(),
                domain: domain.clone(),
                kind: "http-403".to_string(),
                strategy,
                ok: false,
                detail: Some(format!("attempt {}", attempt + 1)),
            });
            last_challenge_body = Some(decode_body(&attempt_resp.body, &attempt_resp.headers));
            last_challenge = Some(status_error(attempt_resp.status, &opts.url));
            continue;
        }

        if attempt > 0 {
            let _ = ctx.store.record_challenge(&cb_store::ChallengeEvent {
                ts: ctx.now(),
                domain: domain.clone(),
                kind: "http-403".to_string(),
                strategy,
                ok: true,
                detail: Some(format!("succeeded on attempt {}", attempt + 1)),
            });
        }
        if let Some(path) = &opts.record {
            client.save_recording(path)?;
        }
        resp = Some(attempt_resp);
        break;
    }

    let resp = match resp {
        Some(r) => r,
        None => {
            let mut err = last_challenge.unwrap_or_else(|| CbError::other("no attempt ran"));
            // 最后一阶：模型看挑战页给建议（默认关闭，显式 --ai-escalate 才走）。
            if opts.ai_escalate {
                if let Some(body) = last_challenge_body {
                    match crate::challenge::llm_advice(ctx, &opts.url, &body).await {
                        Ok(advice) => {
                            let _ = ctx.store.record_challenge(&cb_store::ChallengeEvent {
                                ts: ctx.now(),
                                domain: domain.clone(),
                                kind: "http-403".to_string(),
                                strategy: "llm-assist".to_string(),
                                ok: true,
                                detail: Some(advice.chars().take(200).collect()),
                            });
                            if let Some(needs) = err.body.needs_user.as_mut() {
                                needs.hint = format!("{} | model: {advice}", needs.hint);
                            }
                        }
                        Err(e) => {
                            let _ = ctx.store.record_challenge(&cb_store::ChallengeEvent {
                                ts: ctx.now(),
                                domain: domain.clone(),
                                kind: "http-403".to_string(),
                                strategy: "llm-assist".to_string(),
                                ok: false,
                                detail: Some(e.body.message.chars().take(200).collect()),
                            });
                        }
                    }
                }
            }
            return Err(err);
        }
    };

    let content_type = resp.header("content-type").unwrap_or("").to_string();

    if resp.status >= 400 {
        return Err(status_error(resp.status, &opts.url));
    }
    ensure_text_content_type(&content_type)?;
    if resp.body.len() > MAX_BODY_BYTES {
        return Err(CbError::too_large(format!(
            "body is {} bytes, cap is {} bytes",
            resp.body.len(),
            MAX_BODY_BYTES
        )));
    }
    let body_text = decode_body(&resp.body, &resp.headers);

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
        jar_has_session,
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
            "final_url": resp.final_url,
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
    jar_has_session: bool,
) -> Result<FetchOutcome, CbError> {
    let doc = cb_dom::Document::parse(&html);
    let title = doc.title().unwrap_or_default();
    let extract_opts = ExtractOpts {
        scope: opts.scope.clone(),
        max_tokens: opts.max_tokens,
        images: opts.images,
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
    let bytes = html.len();
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

    // 记正文版本：同一正文不重复记，正文变了才新增一行。
    if !md.text.trim().is_empty() {
        let sha = content_fingerprint(md.text.as_bytes());
        let paragraphs = md
            .text
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count();
        let _ = ctx
            .store
            .save_revision(final_url, &sha, md.text.chars().count(), paragraphs);
    }

    // 学一学这个域名在哪个指纹下能过（site strategy）。
    if let Some(host) = url::Url::parse(final_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
    {
        let _ = ctx.store.record_site(&host, ctx.profile.as_str(), true);
    }

    let render_doc = cb_extract::to_document(&html, &extract_opts).ok();
    let login = crate::login::detect_login_wall(final_url, headers, &html, jar_has_session);

    Ok(FetchOutcome {
        doc: render_doc,
        login,
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
        bytes,
    })
}
/// 正文体积上限：防 500MB 视频/PDF 把内存和库里塞满。
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// 只处理文本类响应；二进制直接拒绝，给出明确错误而不是乱码。
fn ensure_text_content_type(content_type: &str) -> Result<(), CbError> {
    let ct = content_type.to_ascii_lowercase();
    if ct.is_empty()
        || ct.starts_with("text/")
        || ct.contains("html")
        || ct.contains("json")
        || ct.contains("xml")
    {
        return Ok(());
    }
    Err(CbError::other(format!("unsupported content type: {ct}")))
}

fn charset_from_headers(headers: &[(String, String)]) -> Option<String> {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("content-type") {
            let lower = v.to_ascii_lowercase();
            if let Some(pos) = lower.find("charset=") {
                // 支持 charset="gbk" 这种带引号的写法
                let rest = v[pos + 8..].trim_start_matches(['"', '\'']);
                let label = rest
                    .split([';', ' ', '"', '\''])
                    .next()
                    .unwrap_or("")
                    .trim();
                if !label.is_empty() {
                    return Some(label.to_string());
                }
            }
        }
    }
    None
}

/// 没有响应头 charset 时，看 HTML 前 2KB 里的 meta charset。
fn sniff_meta_charset(body: &[u8]) -> Option<String> {
    let head = &body[..body.len().min(2048)];
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let pos = text.find("charset=")?;
    let rest = text[pos + 8..].trim_start_matches(['"', '\'']);
    let label = rest
        .split(['"', '\'', '>', ' ', '/'])
        .next()
        .unwrap_or("")
        .trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

/// 内容指纹（只用于判断正文有没有变，不做安全用途）。
fn content_fingerprint(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 按响应头 charset（其次 meta charset）解码；都没有就按 UTF-8 容错。
/// 中文/日文站点走 GBK/Big5/Shift_JIS 时这一步决定内容是不是乱码。
pub fn decode_body(body: &[u8], headers: &[(String, String)]) -> String {
    let label = charset_from_headers(headers).or_else(|| sniff_meta_charset(body));
    if let Some(label) = label {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            let (decoded, _, _) = encoding.decode(body);
            return decoded.into_owned();
        }
    }
    String::from_utf8_lossy(body).into_owned()
}
/// 指纹阶梯：先用用户选的，再试另外两个。
fn challenge_ladder(preferred: cb_net::Profile) -> Vec<cb_net::Profile> {
    let all = [
        cb_net::Profile::Chrome,
        cb_net::Profile::Firefox,
        cb_net::Profile::Safari,
    ];
    let mut out = vec![preferred];
    for profile in all {
        if profile != preferred {
            out.push(profile);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ct(value: &str) -> Vec<(String, String)> {
        vec![("content-type".to_string(), value.to_string())]
    }

    #[test]
    fn decodes_gbk_when_charset_declared() {
        // "中文" 的 GBK 字节
        let body = [0xD6u8, 0xD0, 0xCE, 0xC4];
        assert_eq!(decode_body(&body, &ct("text/html; charset=gbk")), "中文");
    }

    #[test]
    fn decodes_from_meta_charset_when_header_silent() {
        let mut body = b"<html><head><meta charset=\"gbk\"></head><body>".to_vec();
        body.extend_from_slice(&[0xD6u8, 0xD0, 0xCE, 0xC4]);
        body.extend_from_slice(b"</body></html>");
        let text = decode_body(&body, &ct("text/html"));
        assert!(text.contains("中文"), "got: {text}");
    }

    #[test]
    fn rejects_binary_content_types() {
        assert!(ensure_text_content_type("image/png").is_err());
        assert!(ensure_text_content_type("application/pdf").is_err());
        assert!(ensure_text_content_type("text/html; charset=utf-8").is_ok());
        assert!(ensure_text_content_type("application/json").is_ok());
    }

    #[test]
    fn cache_ttl_honours_no_store_and_max_age() {
        assert_eq!(
            cache_ttl_secs(
                &ct("text/html")
                    .into_iter()
                    .chain(vec![(
                        "cache-control".to_string(),
                        "max-age=60".to_string()
                    )])
                    .collect::<Vec<_>>()
            ),
            60
        );
        assert_eq!(
            cache_ttl_secs(&[("cache-control".to_string(), "no-store".to_string())]),
            0
        );
    }
    #[tokio::test]
    async fn login_wall_reports_expired_session_when_jar_has_cookies() {
        use cb_net::cookies::Cookie;
        let dir = tempfile::tempdir().expect("tempdir");
        let cassette = dir.path().join("login.cassette.json");
        let html = "<html><head><title>Sign in</title></head><body><main><h1>Sign in</h1><form><input type=\"password\"></form><p>Sign in to continue to your account dashboard and settings page please now.</p></main></body></html>";
        let payload = serde_json::json!({
            "version": 1,
            "entries": [{
                "url": "https://site.example/login",
                "final_url": "https://site.example/login",
                "status": 200,
                "headers": {"content-type": "text/html; charset=utf-8"},
                "body": html,
            }]
        });
        std::fs::write(
            &cassette,
            serde_json::to_string_pretty(&payload).expect("json"),
        )
        .expect("write");

        let store = cb_store::Store::open_in_memory().expect("store");
        let ctx = crate::Ctx::new(store, cb_net::Profile::Chrome);
        ctx.cookies.save(&[Cookie {
            domain: "site.example".to_string(),
            name: "sessionid".to_string(),
            path: "/".to_string(),
            value: "abc".to_string(),
            expires_at: None,
            secure: false,
            http_only: true,
            same_site: None,
            host_only: true,
        }]);

        let opts = FetchOpts {
            url: "https://site.example/login".to_string(),
            replay: Some(cassette),
            ..Default::default()
        };
        let outcome = fetch_page(&ctx, &opts).await.expect("fetch");
        let wall = outcome.login.expect("login wall");
        assert!(
            wall.hints
                .iter()
                .any(|h| h.contains("expired") || h.contains("refresh")),
            "hints: {:?}",
            wall.hints
        );
    }
}
