//! 搜索引擎适配：把 SERP 解析成结构化结果，给 CLI / MCP / agent 共用。
//! Search adapters: turn SERPs into structured results.

use crate::error::CbError;
use crate::Ctx;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    DuckDuckGo,
    Bing,
    Github,
    Arxiv,
}

impl Engine {
    pub fn parse(raw: &str) -> Option<Engine> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ddg" | "duckduckgo" | "duck" => Some(Engine::DuckDuckGo),
            "bing" => Some(Engine::Bing),
            "gh" | "github" => Some(Engine::Github),
            "arxiv" | "ax" => Some(Engine::Arxiv),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "ddg",
            Engine::Bing => "bing",
            Engine::Github => "gh",
            Engine::Arxiv => "arxiv",
        }
    }

    /// 构造查询 URL。
    pub fn url(self, query: &str, limit: usize) -> String {
        let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        match self {
            Engine::DuckDuckGo => format!("https://html.duckduckgo.com/html/?q={q}"),
            Engine::Bing => format!("https://www.bing.com/search?q={q}&count={limit}"),
            Engine::Github => {
                format!("https://api.github.com/search/repositories?q={q}&per_page={limit}")
            }
            Engine::Arxiv => format!(
                "https://export.arxiv.org/api/query?search_query=all:{q}&max_results={limit}"
            ),
        }
    }

    pub fn all() -> &'static [Engine] {
        &[
            Engine::DuckDuckGo,
            Engine::Bing,
            Engine::Github,
            Engine::Arxiv,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct SearchOpts {
    pub engine: Engine,
    pub query: String,
    pub limit: usize,
    pub replay: Option<PathBuf>,
    pub timeout_ms: u64,
}

impl SearchOpts {
    pub fn new(engine: Engine, query: impl Into<String>) -> SearchOpts {
        SearchOpts {
            engine,
            query: query.into(),
            limit: 10,
            replay: None,
            timeout_ms: 30_000,
        }
    }
}

fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn parse_ddg(html: &str, limit: usize) -> Vec<SearchResult> {
    let doc = cb_dom::Document::parse(html);
    let mut out = Vec::new();
    for block in doc.select(".result") {
        let title_el = doc.select("a.result__a").into_iter().find(|a| {
            block
                .select(&selector("a.result__a"))
                .any(|x| x.id() == a.id())
        });
        let Some(title_el) = title_el else { continue };
        let title = clean(&title_el.text().collect::<Vec<_>>().join(" "));
        let url = title_el
            .value()
            .attr("href")
            .unwrap_or_default()
            .to_string();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = block
            .select(&selector(".result__snippet"))
            .next()
            .map(|s| clean(&s.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        out.push(SearchResult {
            title,
            url: unwrap_ddg_redirect(&url),
            snippet,
            source: "ddg".to_string(),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn selector(css: &str) -> scraper::Selector {
    scraper::Selector::parse(css).expect("static selector")
}

/// DDG 的链接有时是 /l/?uddg=<encoded>，还原出真实地址。
fn unwrap_ddg_redirect(url: &str) -> String {
    if let Some(pos) = url.find("uddg=") {
        let raw = &url[pos + 5..];
        let raw = raw.split('&').next().unwrap_or(raw);
        if let Ok(decoded) = url::form_urlencoded::parse(raw.as_bytes())
            .next()
            .map(|(v, _)| v.into_owned())
            .ok_or(())
        {
            return decoded;
        }
    }
    url.to_string()
}

fn parse_bing(html: &str, limit: usize) -> Vec<SearchResult> {
    let doc = cb_dom::Document::parse(html);
    let mut out = Vec::new();
    for block in doc.select("li.b_algo") {
        let Some(anchor) = block.select(&selector("h2 a")).next() else {
            continue;
        };
        let title = clean(&anchor.text().collect::<Vec<_>>().join(" "));
        let url = anchor.value().attr("href").unwrap_or_default().to_string();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = block
            .select(&selector("p"))
            .next()
            .map(|s| clean(&s.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        out.push(SearchResult {
            title,
            url,
            snippet,
            source: "bing".to_string(),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn parse_github(json: &str, limit: usize) -> Result<Vec<SearchResult>, CbError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| CbError::other(format!("github returned invalid json: {e}")))?;
    let items = value["items"]
        .as_array()
        .ok_or_else(|| CbError::other("github returned no items (rate limited?)".to_string()))?;
    let mut out = Vec::new();
    for item in items.iter().take(limit) {
        let name = item["full_name"].as_str().unwrap_or_default().to_string();
        let url = item["html_url"].as_str().unwrap_or_default().to_string();
        if name.is_empty() || url.is_empty() {
            continue;
        }
        let mut snippet = item["description"].as_str().unwrap_or_default().to_string();
        if let Some(stars) = item["stargazers_count"].as_i64() {
            snippet = format!("{snippet} · ★{stars}");
        }
        out.push(SearchResult {
            title: name,
            url,
            snippet: clean(&snippet),
            source: "gh".to_string(),
        });
    }
    Ok(out)
}

fn parse_arxiv(xml: &str, limit: usize) -> Vec<SearchResult> {
    let doc = cb_dom::Document::parse(xml);
    let mut out = Vec::new();
    for entry in doc.select("entry") {
        let title = entry
            .select(&selector("title"))
            .next()
            .map(|t| clean(&t.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        let url = entry
            .select(&selector("id"))
            .next()
            .map(|t| clean(&t.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = entry
            .select(&selector("summary"))
            .next()
            .map(|t| clean(&t.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        out.push(SearchResult {
            title,
            url,
            snippet: snippet.chars().take(600).collect(),
            source: "arxiv".to_string(),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub fn parse_results(
    engine: Engine,
    body: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, CbError> {
    Ok(match engine {
        Engine::DuckDuckGo => parse_ddg(body, limit),
        Engine::Bing => parse_bing(body, limit),
        Engine::Github => parse_github(body, limit)?,
        Engine::Arxiv => parse_arxiv(body, limit),
    })
}

/// 跑一次搜索：网络（或回放）→ 解析 → 结构化结果。
pub async fn search(ctx: &Ctx, opts: &SearchOpts) -> Result<Vec<SearchResult>, CbError> {
    if opts.query.trim().is_empty() {
        return Err(CbError::usage("query is required"));
    }
    let url = opts.engine.url(&opts.query, opts.limit);
    let client = match &opts.replay {
        Some(path) => cb_net::NetClient::replay(path)?,
        None => cb_net::NetClient::new(ctx.profile)?,
    };
    let request = cb_net::Request::get(url.clone())
        .header("accept", "text/html,application/json,application/atom+xml");
    let resp = match tokio::time::timeout(
        std::time::Duration::from_millis(opts.timeout_ms),
        client.get(&request),
    )
    .await
    {
        Err(_) => {
            return Err(CbError::timeout(format!(
                "search timed out after {}ms",
                opts.timeout_ms
            )))
        }
        Ok(Err(e)) => return Err(e.into()),
        Ok(Ok(r)) => r,
    };
    if resp.status >= 400 {
        return Err(match resp.status {
            403 | 429 => CbError::challenge(
                format!("{} blocked with http {}", opts.engine.as_str(), resp.status),
                &url,
                "rotate profile, slow down, or hand off to the calling agent",
            ),
            404 => CbError::not_found(format!("{} endpoint 404", opts.engine.as_str())),
            s => CbError::network(format!("{} http {s}", opts.engine.as_str())),
        });
    }
    let body = crate::fetch::decode_body(&resp.body, &resp.headers);
    let results = parse_results(opts.engine, &body, opts.limit)?;

    ctx.store
        .append_history(&cb_store::HistoryRow {
            id: 0,
            ts: ctx.now(),
            url,
            title: format!("{}: {}", opts.engine.as_str(), opts.query),
            kind: "search".to_string(),
            session_id: None,
            duration_ms: 0,
            ok: true,
        })
        .map_err(crate::error::store_err)?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_parsing_and_urls() {
        assert_eq!(Engine::parse("ddg"), Some(Engine::DuckDuckGo));
        assert_eq!(Engine::parse("GitHub"), Some(Engine::Github));
        assert!(Engine::parse("altavista").is_none());
        assert!(Engine::Github.url("rust http", 5).contains("per_page=5"));
        assert!(Engine::Arxiv.url("diffusion", 3).contains("max_results=3"));
        assert!(Engine::DuckDuckGo.url("a b", 10).contains("q=a+b"));
    }

    #[test]
    fn ddg_parses_and_unwraps_redirects() {
        let html = r#"<div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&x=1">Example A</a><a class="result__snippet">snippet a</a></div>"#;
        let results = parse_results(Engine::DuckDuckGo, html, 10).expect("parse");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/a");
        assert_eq!(results[0].title, "Example A");
        assert_eq!(results[0].snippet, "snippet a");
    }

    #[test]
    fn bing_parses_algo_blocks() {
        let html = r#"<li class="b_algo"><h2><a href="https://example.com/b">Example B</a></h2><p>snippet b</p></li>"#;
        let results = parse_results(Engine::Bing, html, 10).expect("parse");
        assert_eq!(results[0].url, "https://example.com/b");
        assert_eq!(results[0].snippet, "snippet b");
    }

    #[test]
    fn github_parses_json() {
        let json = r#"{"items":[{"full_name":"rust-lang/rust","html_url":"https://github.com/rust-lang/rust","description":"the language","stargazers_count":90000}]}"#;
        let results = parse_results(Engine::Github, json, 10).expect("parse");
        assert_eq!(results[0].title, "rust-lang/rust");
        assert!(results[0].snippet.contains("★90000"));
    }

    #[test]
    fn arxiv_parses_atom() {
        let xml = r#"<feed><entry><title>Diffusion Models</title><id>https://arxiv.org/abs/2006.11239</id><summary>We study diffusion.</summary></entry></feed>"#;
        let results = parse_results(Engine::Arxiv, xml, 10).expect("parse");
        assert_eq!(results[0].title, "Diffusion Models");
        assert!(results[0].url.contains("2006.11239"));
    }
}
