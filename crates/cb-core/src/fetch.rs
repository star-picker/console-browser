//! 取页面主流程：缓存 → 网络 → DOM → 提取 → 存库 → 历史。

use crate::error::CbError;
use crate::Ctx;
use cb_extract::{ExtractOpts, Markdown};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 从插件目录找命中当前域名、且有 extract 权限的抽取器，返回其第一个选择器。
fn plugin_extractor_scope(html: &str, url: &str) -> Option<String> {
    let dir = std::env::var("CB_PLUGIN_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(cb_plugins::default_plugin_dir);
    let host = url::Url::parse(url).ok()?.host_str()?.to_string();
    let selection = cb_plugins::extractor_for_host(&dir, &host)?;
    // 先试 JS 钩子：脚本拿页面 DOM 快照，返回值当作选择器。
    if let Some(script_path) = &selection.script {
        if let Ok(code) = std::fs::read_to_string(script_path) {
            if let Ok(engine) = cb_js::JsRuntime::new() {
                if engine.install_html(html).is_ok() {
                    if let Ok(out) = engine.eval_with_timeout(&code, 2_000) {
                        let sel = out.trim().trim_matches('"').trim().to_string();
                        if !sel.is_empty() {
                            return Some(sel);
                        }
                    }
                }
            }
        }
    }
    selection.selectors.into_iter().next()
}

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
    /// 跨多次抓取共享回放游标（bench 用；普通单次抓取留空即可）。
    pub replay_session: Option<cb_net::ReplaySession>,
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
            replay_session: None,
            record: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            ai_escalate: false,
            images: true,
        }
    }
}

/// 一次抓取尝试的回执：给调用方看「换过指纹没有、每次多久、错在哪」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchAttempt {
    pub profile: String,
    pub ok: bool,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FetchAttempt {
    pub fn failed(profile: cb_net::Profile, ms: u64, err: &CbError) -> FetchAttempt {
        FetchAttempt {
            profile: profile.as_str().to_string(),
            ok: false,
            ms,
            error: Some(err.body.code.clone()),
        }
    }

    pub fn label(&self) -> String {
        format!(
            "{}:{}",
            self.profile,
            self.error.clone().unwrap_or_else(|| "ok".to_string())
        )
    }
}

/// 三段超时：连接 5s，读取 25s，总预算跟随 --timeout。
pub fn fetch_timeouts(total_ms: u64) -> cb_net::Timeouts {
    let total = std::time::Duration::from_millis(total_ms.max(1));
    let connect = std::time::Duration::from_millis(total_ms.clamp(1_000, 5_000));
    let read = std::time::Duration::from_millis(total_ms.saturating_sub(5_000).max(1_000));
    cb_net::Timeouts {
        connect,
        read,
        total: Some(total),
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
    /// 正文容器得分（meta.extract.container_score 的来源，不进 data，避免重复事实）。
    #[serde(skip)]
    pub container_score: Option<f64>,
    /// --scope 回执（meta.extract.scope 的来源，不进 data）。
    #[serde(skip)]
    pub scope: Option<cb_extract::ScopeReport>,
    /// 选中的容器是不是语义化正文容器（meta.extract.is_article 的来源）。
    #[serde(skip)]
    pub is_article: bool,
    /// 试过哪些指纹、各自耗时（meta.attempts 的来源，不进 data）。
    #[serde(skip)]
    pub attempts: Vec<FetchAttempt>,
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

/// Retry-After 头：秒数；没有就按 5s 建议（429 的常见退避起点）。
pub fn retry_after_s(headers: &[(String, String)]) -> u64 {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("retry-after") {
            if let Ok(n) = v.trim().parse::<u64>() {
                return n;
            }
        }
    }
    5
}

fn status_error(status: u16, url: &str, headers: &[(String, String)]) -> CbError {
    match status {
        401 | 407 => CbError::needs_auth(format!("http {status}"), url),
        403 => CbError::challenge(
            format!("http {status}: looks like an anti-bot response"),
            url,
            "escalate: rotate profile, warm cookies, or hand off to the calling agent",
        ),
        404 | 410 => CbError::not_found(format!("http {status}")),
        // 429 不是反爬：它明确说「慢一点」，可重试并带建议等待时长。
        429 => CbError::rate_limited(
            "http 429: too many requests (rate limited)".to_string(),
            Some(retry_after_s(headers)),
        ),
        // 5xx 是瞬时的：可重试，交给调用方决定。
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

    // 失败阶梯：换指纹 + 指数退避，全程留痕。
    // 只重试 network/timeout；404/429/DNS/403 立即换指纹或直接失败。
    let mut resp = None;
    let mut last_error: Option<CbError> = None;
    let mut last_challenge_body: Option<String> = None;
    let mut attempts: Vec<FetchAttempt> = Vec::new();
    let ladder = profile_ladder(ctx.profile);
    let backoff_ms = [500u64, 1500u64];
    let timeouts = fetch_timeouts(opts.timeout_ms);
    let all_started = std::time::Instant::now();
    // 回放客户端只建一次：同一 URL 的多条 cassette 条目按尝试顺序消费。
    let replay_client = match &opts.replay {
        Some(path) => Some(cb_net::NetClient::replay_with_session(
            path,
            opts.replay_session.clone().unwrap_or_default(),
        )?),
        None => None,
    };
    for (attempt, profile) in ladder.iter().copied().enumerate() {
        let client = match &replay_client {
            Some(c) => c.clone(),
            None => cb_net::NetClient::new_with_timeouts(profile, timeouts)?
                .with_cookie_store(ctx.cookies.clone()),
        };
        if let Some(path) = &opts.record {
            client.start_recording();
            tracing::debug!(path = %path.display(), "recording enabled");
        }
        let request = cb_net::Request::get(opts.url.clone());
        let started = std::time::Instant::now();
        let sent = tokio::time::timeout(
            std::time::Duration::from_millis(opts.timeout_ms),
            client.get(&request),
        )
        .await;
        let ms = started.elapsed().as_millis() as u64;
        let result: Result<cb_net::Response, CbError> = match sent {
            Err(_) => Err(CbError::timeout(format!(
                "no response within {}ms",
                opts.timeout_ms
            ))),
            Ok(Err(e)) => Err(e.into()),
            Ok(Ok(r)) => Ok(r),
        };

        match result {
            Err(err) => {
                attempts.push(FetchAttempt::failed(profile, ms, &err));
                let retry = err.body.retryable;
                last_error = Some(err);
                if retry && attempt + 1 < ladder.len() {
                    let wait = backoff_ms[attempt.min(backoff_ms.len() - 1)];
                    tracing::debug!(
                        profile = profile.as_str(),
                        wait_ms = wait,
                        "retrying after retryable failure"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                    continue;
                }
                break;
            }
            Ok(attempt_resp) if attempt_resp.status == 403 => {
                let strategy = format!("rotate-profile:{}", profile.as_str());
                let _ = ctx.store.record_challenge(&cb_store::ChallengeEvent {
                    ts: ctx.now(),
                    domain: domain.clone(),
                    kind: "http-403".to_string(),
                    strategy,
                    ok: false,
                    detail: Some(format!("attempt {}", attempt + 1)),
                });
                last_challenge_body = Some(decode_body(&attempt_resp.body, &attempt_resp.headers));
                let err = status_error(attempt_resp.status, &opts.url, &attempt_resp.headers);
                attempts.push(FetchAttempt::failed(profile, ms, &err));
                last_error = Some(err);
                // 403 换指纹继续；挑战不是「太快了」，不加退避。
                if attempt + 1 < ladder.len() {
                    continue;
                }
                break;
            }
            Ok(attempt_resp) if attempt_resp.status >= 400 => {
                let err = status_error(attempt_resp.status, &opts.url, &attempt_resp.headers);
                attempts.push(FetchAttempt::failed(profile, ms, &err));
                last_error = Some(err);
                // 404/429 这类状态码重试也是同一个答案，直接失败。
                break;
            }
            Ok(attempt_resp) => {
                attempts.push(FetchAttempt {
                    profile: profile.as_str().to_string(),
                    ok: true,
                    ms,
                    error: None,
                });
                if attempt > 0 {
                    let strategy = format!("rotate-profile:{}", profile.as_str());
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
        }
    }

    let resp = match resp {
        Some(r) => r,
        None => {
            let mut err = last_error.unwrap_or_else(|| CbError::other("no attempt ran"));
            // 全部失败时把「试过谁、各自错在哪」写进 message，读不到 attempts 的调用方也能懂。
            if attempts.len() > 1 {
                let summary = attempts
                    .iter()
                    .map(FetchAttempt::label)
                    .collect::<Vec<_>>()
                    .join(", ");
                err.body.message = format!(
                    "all {} profiles failed after {:.0}s ({summary}): {}",
                    attempts.len(),
                    all_started.elapsed().as_secs_f64(),
                    err.body.message
                );
            }
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
            err.body.attempts = attempts;
            return Err(err);
        }
    };

    let content_type = resp.header("content-type").unwrap_or("").to_string();

    if resp.status >= 400 {
        return Err(status_error(resp.status, &opts.url, &resp.headers));
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

    let mut outcome = build_outcome(
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
    outcome.attempts = attempts;

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
    // 插件声明式抽取器：用户没显式指定 --scope 时，命中域名的插件接管选择器。
    let plugin_scope = if opts.scope.is_none() {
        plugin_extractor_scope(&html, final_url)
    } else {
        None
    };
    let extract_opts = ExtractOpts {
        scope: opts.scope.clone().or(plugin_scope),
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
        container_score: md.container_score,
        scope: md.scope.clone(),
        is_article: md.is_article,
        attempts: Vec::new(),
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
/// 指纹阶梯：先用用户选的（默认 chrome），再试另外两个。
fn profile_ladder(preferred: cb_net::Profile) -> Vec<cb_net::Profile> {
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

    #[test]
    fn timeouts_split_connect_read_and_total() {
        let t = fetch_timeouts(DEFAULT_TIMEOUT_MS);
        assert_eq!(t.connect, std::time::Duration::from_secs(5));
        assert_eq!(t.read, std::time::Duration::from_secs(25));
        assert_eq!(
            t.total,
            Some(std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS))
        );

        // --timeout 覆盖总预算：连接仍然最多 5s，读取吃掉剩余的。
        let long = fetch_timeouts(60_000);
        assert_eq!(long.connect, std::time::Duration::from_secs(5));
        assert_eq!(long.read, std::time::Duration::from_secs(55));
        assert_eq!(long.total, Some(std::time::Duration::from_secs(60)));
    }

    #[test]
    fn profile_ladder_starts_with_requested_then_others() {
        use cb_net::Profile;
        assert_eq!(
            profile_ladder(Profile::Chrome),
            vec![Profile::Chrome, Profile::Firefox, Profile::Safari]
        );
        assert_eq!(
            profile_ladder(Profile::Safari),
            vec![Profile::Safari, Profile::Chrome, Profile::Firefox],
            "explicit profile goes first, then the rest"
        );
    }
}
