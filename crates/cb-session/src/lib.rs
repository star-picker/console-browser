//! cb-session: 常驻 daemon 的会话状态 + 页面句柄（@1/@2）+ 快照差量。
//!
//! 协议是换行分隔的 JSON：请求 {id, method, params}，响应 {id, ok, result|error}。
//! 句柄只活在 daemon 里；客户端（CLI / 未来 cb-js）通过传输层问它要快照。

use cb_core::error::ErrorBody;
use cb_core::{CbError, Ctx};
use cb_extract::{Block, Span};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod transport;

pub const DEFAULT_PIPE: &str = "cb-daemon";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(method: impl Into<String>, params: Value) -> Request {
        Request {
            id: 1,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Response {
        Response {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, err: CbError) -> Response {
        Response {
            id,
            ok: false,
            result: None,
            error: Some(err.body),
        }
    }

    pub fn usage(id: u64, message: impl Into<String>) -> Response {
        Response::err(id, CbError::usage(message))
    }
}

/// 快照形态：full 带全部 blocks；delta 只带 ops；unchanged 什么都不用改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Full,
    Delta,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BlockOp {
    Replace { index: usize, block: Block },
    Insert { index: usize, block: Block },
    Remove { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotView {
    pub handle: String,
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub revision: u64,
    pub total: usize,
    pub kind: SnapshotKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<Block>>,
    #[serde(default)]
    pub ops: Vec<BlockOp>,
    pub cached: bool,
    pub status: u16,
    pub tokens: usize,
    pub fetched_at: i64,
}

#[derive(Debug, Clone)]
struct PageData {
    final_url: String,
    title: String,
    blocks: Vec<Block>,
    status: u16,
    cached: bool,
    tokens: usize,
    fetched_at: i64,
}

#[derive(Debug, Clone)]
struct Page {
    handle: String,
    url: String,
    revision: u64,
    current: PageData,
    previous: Option<PageData>,
}

impl Page {
    fn view(&self, since: Option<u64>) -> SnapshotView {
        let mut view = SnapshotView {
            handle: self.handle.clone(),
            url: self.url.clone(),
            final_url: self.current.final_url.clone(),
            title: self.current.title.clone(),
            revision: self.revision,
            total: self.current.blocks.len(),
            kind: SnapshotKind::Full,
            blocks: None,
            ops: Vec::new(),
            cached: self.current.cached,
            status: self.current.status,
            tokens: self.current.tokens,
            fetched_at: self.current.fetched_at,
        };
        match since {
            Some(rev) if rev == self.revision => {
                view.kind = SnapshotKind::Unchanged;
            }
            Some(rev) if rev.checked_add(1) == Some(self.revision) && self.previous.is_some() => {
                let previous = self.previous.as_ref().expect("checked");
                view.kind = SnapshotKind::Delta;
                view.ops = diff_blocks(&previous.blocks, &self.current.blocks);
            }
            _ => {
                view.kind = SnapshotKind::Full;
                view.blocks = Some(self.current.blocks.clone());
            }
        }
        view
    }
}

/// 块级差量：共同前缀/后缀之外的部分。
///
/// 中段等长就逐个 replace；长度不同就删掉旧中段再按顺序插入新中段。
pub fn diff_blocks(old: &[Block], new: &[Block]) -> Vec<BlockOp> {
    if old == new {
        return Vec::new();
    }
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    let mut ops = Vec::new();
    if old_mid.len() == new_mid.len() {
        for (i, block) in new_mid.iter().enumerate() {
            ops.push(BlockOp::Replace {
                index: prefix + i,
                block: block.clone(),
            });
        }
        return ops;
    }
    for index in (prefix..prefix + old_mid.len()).rev() {
        ops.push(BlockOp::Remove { index });
    }
    for (i, block) in new_mid.iter().enumerate() {
        ops.push(BlockOp::Insert {
            index: prefix + i,
            block: block.clone(),
        });
    }
    ops
}

/// 常驻会话表：句柄只增不减；每个页面保留上一版用于差量。
pub struct Daemon {
    ctx: Ctx,
    replay: Option<PathBuf>,
    pages: BTreeMap<String, Page>,
    next_handle: u32,
}

impl Daemon {
    pub fn new(ctx: Ctx, replay: Option<PathBuf>) -> Daemon {
        Daemon {
            ctx,
            replay,
            pages: BTreeMap::new(),
            next_handle: 0,
        }
    }

    pub fn handle_count(&self) -> usize {
        self.pages.len()
    }

    pub async fn call(&mut self, req: Request) -> Response {
        let id = req.id;
        match req.method.as_str() {
            "ping" => Response::ok(
                id,
                json!({
                    "pong": true,
                    "version": cb_core::ENGINE,
                    "sessions": self.pages.len(),
                }),
            ),
            "open" => self.open(id, &req.params).await,
            "refresh" => self.refresh(id, &req.params).await,
            "get" => self.get(id, &req.params),
            "list" => self.list(id),
            "close" => self.close(id, &req.params),
            other => Response::usage(id, format!("unknown method: {other}")),
        }
    }

    async fn open(&mut self, id: u64, params: &Value) -> Response {
        let Some(url) = param_str(params, "url") else {
            return Response::usage(id, "open requires params.url");
        };
        let fresh = param_bool(params, "fresh");
        let data = match self.fetch(url, fresh).await {
            Ok(data) => data,
            Err(err) => return Response::err(id, err),
        };
        self.next_handle += 1;
        let handle = format!("@{}", self.next_handle);
        let page = Page {
            handle: handle.clone(),
            url: url.to_string(),
            revision: 1,
            current: data,
            previous: None,
        };
        let view = page.view(None);
        self.pages.insert(handle, page);
        Response::ok(id, to_value(&view))
    }

    async fn refresh(&mut self, id: u64, params: &Value) -> Response {
        let Some(handle) = param_str(params, "handle") else {
            return Response::usage(id, "refresh requires params.handle");
        };
        let Some(url) = self.pages.get(handle).map(|p| p.url.clone()) else {
            return Response::err(id, CbError::not_found(format!("unknown handle: {handle}")));
        };
        let fresh = param_bool(params, "fresh");
        let data = match self.fetch(&url, fresh).await {
            Ok(data) => data,
            Err(err) => return Response::err(id, err),
        };
        let since = params.get("since").and_then(Value::as_u64);
        let page = self.pages.get_mut(handle).expect("checked above");
        page.previous = Some(page.current.clone());
        page.current = data;
        page.revision += 1;
        let view = page.view(since);
        Response::ok(id, to_value(&view))
    }

    fn get(&self, id: u64, params: &Value) -> Response {
        let Some(handle) = param_str(params, "handle") else {
            return Response::usage(id, "get requires params.handle");
        };
        let Some(page) = self.pages.get(handle) else {
            return Response::err(id, CbError::not_found(format!("unknown handle: {handle}")));
        };
        let since = params.get("since").and_then(Value::as_u64);
        Response::ok(id, to_value(&page.view(since)))
    }

    fn list(&self, id: u64) -> Response {
        let sessions: Vec<Value> = self
            .pages
            .values()
            .map(|page| {
                json!({
                    "handle": page.handle,
                    "url": page.url,
                    "title": page.current.title,
                    "revision": page.revision,
                    "blocks": page.current.blocks.len(),
                    "cached": page.current.cached,
                })
            })
            .collect();
        Response::ok(id, json!({ "sessions": sessions }))
    }

    fn close(&mut self, id: u64, params: &Value) -> Response {
        let Some(handle) = param_str(params, "handle") else {
            return Response::usage(id, "close requires params.handle");
        };
        match self.pages.remove(handle) {
            Some(_) => Response::ok(id, json!({ "closed": handle })),
            None => Response::err(id, CbError::not_found(format!("unknown handle: {handle}"))),
        }
    }

    async fn fetch(&self, url: &str, fresh: bool) -> Result<PageData, CbError> {
        let opts = cb_core::FetchOpts {
            url: url.to_string(),
            replay: self.replay.clone(),
            fresh,
            ..Default::default()
        };
        let outcome = cb_core::fetch_page(&self.ctx, &opts).await?;
        let blocks = outcome
            .doc
            .as_ref()
            .map(|doc| doc.blocks.clone())
            .filter(|blocks| !blocks.is_empty())
            .unwrap_or_else(|| {
                vec![Block::Paragraph {
                    spans: vec![Span::normal(outcome.markdown.clone())],
                }]
            });
        Ok(PageData {
            final_url: outcome.final_url,
            title: outcome.title,
            blocks,
            status: outcome.status,
            cached: outcome.cached,
            tokens: outcome.tokens_estimate,
            fetched_at: cb_core::now_secs(),
        })
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn param_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn param_bool(params: &Value, key: &str) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// 给 CLI 的人读渲染：块列表 → 纯文本。
pub fn blocks_to_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(block_to_text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Heading { level, spans } => {
            format!("{} {}", "#".repeat(*level as usize), spans_text(spans))
        }
        Block::Paragraph { spans } => spans_text(spans),
        Block::ListItem {
            depth,
            marker,
            spans,
        } => format!(
            "{}{} {}",
            "  ".repeat(*depth as usize),
            marker,
            spans_text(spans)
        ),
        Block::Code { lang, lines } => {
            let body = lines.join("\n");
            if lang.is_empty() {
                format!("```\n{body}\n```")
            } else {
                format!("```{lang}\n{body}\n```")
            }
        }
        Block::Quote { blocks } => blocks
            .iter()
            .map(block_to_text)
            .collect::<Vec<_>>()
            .join("\n")
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Table { head, rows } => {
            let mut out = format!("| {} |", head.join(" | "));
            for row in rows {
                out.push('\n');
                out.push_str(&format!("| {} |", row.join(" | ")));
            }
            out
        }
        Block::Rule => "---".to_string(),
        Block::Image { alt, url } => {
            let label = if alt.trim().is_empty() { url } else { alt };
            format!("![{label}]({url})")
        }
        Block::Notice {
            level,
            title,
            spans,
        } => format!(
            "[{}] {} {}",
            format!("{level:?}").to_lowercase(),
            title,
            spans_text(spans)
        ),
    }
}

/// 单块压成一行，给 delta 的人读输出用。
pub fn block_inline(block: &Block, max_chars: usize) -> String {
    let text = block_to_text(block).replace("\n", " ");
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// delta 的人读摘要。
pub fn ops_to_text(ops: &[BlockOp]) -> String {
    if ops.is_empty() {
        return "unchanged".to_string();
    }
    ops.iter()
        .map(|op| match op {
            BlockOp::Replace { index, block } => {
                format!("~ {index}  {}", block_inline(block, 120))
            }
            BlockOp::Insert { index, block } => {
                format!("+ {index}  {}", block_inline(block, 120))
            }
            BlockOp::Remove { index } => format!("- {index}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
