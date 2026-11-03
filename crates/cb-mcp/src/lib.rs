//! cb-mcp: 手写 JSON-RPC 2.0 over stdio 的 MCP 服务端。
//! 只实现需要的子集：initialize / tools/list / tools/call / ping。
#![allow(clippy::result_large_err)]

use cb_core::fetch::Format;
use cb_core::{registry, CbError, Ctx, FetchOpts};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: &str = "2024-11-05";

fn tool_list() -> Value {
    let tools: Vec<Value> = registry()
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.summary_for(cb_core::i18n::current()),
                "inputSchema": t.input_schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn text_result(text: String, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

async fn call_tool(ctx: &Ctx, replay: Option<&std::path::Path>, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "browse_url" => {
            let Some(url) = args["url"].as_str().map(|s| s.to_string()) else {
                return text_result("usage: url is required".to_string(), true);
            };
            let format = args["format"]
                .as_str()
                .and_then(Format::parse)
                .unwrap_or(Format::Md);
            let opts = FetchOpts {
                url,
                format,
                max_tokens: args["max_tokens"].as_u64().map(|n| n as usize),
                scope: args["scope"].as_str().map(|s| s.to_string()),
                fresh: args["fresh"].as_bool().unwrap_or(false),
                ai_escalate: args["ai_escalate"].as_bool().unwrap_or(false),
                replay: replay.map(|p| p.to_path_buf()),
                ..Default::default()
            };
            match cb_core::fetch_page(ctx, &opts).await {
                Ok(outcome) => {
                    let text = match format {
                        Format::Html => outcome.raw,
                        Format::Json => serde_json::to_string_pretty(&outcome)
                            .unwrap_or_else(|_| outcome.markdown.clone()),
                        _ => outcome.markdown,
                    };
                    text_result(text, false)
                }
                Err(err) => {
                    let hint = err
                        .body
                        .needs_user
                        .as_ref()
                        .map(|needs| format!(" | hint: {}", needs.hint))
                        .unwrap_or_default();
                    text_result(
                        format!("{}: {}{}", err.body.code, err.body.message, hint),
                        true,
                    )
                }
            }
        }
        "search_web" => {
            let Some(query) = args["query"].as_str().map(|s| s.to_string()) else {
                return text_result("usage: query is required".to_string(), true);
            };
            let engine = args["engine"]
                .as_str()
                .and_then(cb_core::Engine::parse)
                .unwrap_or(cb_core::Engine::DuckDuckGo);
            let mut search_opts = cb_core::SearchOpts::new(engine, query);
            search_opts.limit = args["limit"].as_u64().unwrap_or(10) as usize;
            search_opts.replay = replay.map(|p| p.to_path_buf());
            match cb_core::search(ctx, &search_opts).await {
                Ok(results) => text_result(
                    serde_json::to_string_pretty(&json!({ "results": results }))
                        .unwrap_or_else(|_| "{}".to_string()),
                    false,
                ),
                Err(err) => text_result(format!("{}: {}", err.body.code, err.body.message), true),
            }
        }
        "history_search" => {
            let query = args["query"].as_str();
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            match ctx.store.list_history(query, limit) {
                Ok(rows) => text_result(
                    serde_json::to_string_pretty(&json!({ "entries": rows }))
                        .unwrap_or_else(|_| "[]".to_string()),
                    false,
                ),
                Err(e) => text_result(format!("store: {e}"), true),
            }
        }
        "ask_model" => {
            let Some(prompt) = args["prompt"].as_str().map(|s| s.to_string()) else {
                return text_result("usage: prompt is required".to_string(), true);
            };
            let max_tokens = args["max_tokens"].as_u64().unwrap_or(400);
            match cb_core::ai::complete(ctx, "ask", &prompt, max_tokens).await {
                Ok(completion) => text_result(completion.text, false),
                // 本地没 key：借调用方的模型（MCP sampling）。
                Err(err) if err.body.code == "ai_not_configured" => match &ctx.sampler {
                    Some(sampler) => match sampler.ask(&prompt).await {
                        Ok(text) => text_result(text, false),
                        Err(err) => {
                            text_result(format!("{}: {}", err.body.code, err.body.message), true)
                        }
                    },
                    None => text_result(
                        format!(
                            "{}: {} (client did not advertise sampling)",
                            err.body.code, err.body.message
                        ),
                        true,
                    ),
                },
                Err(err) => text_result(format!("{}: {}", err.body.code, err.body.message), true),
            }
        }
        "selftest_status" => {
            let report = cb_core::run_selftest(ctx);
            text_result(
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                false,
            )
        }
        other => text_result(format!("unknown tool: {other}"), true),
    }
}

/// sampling/createMessage 的响应里取文本：兼容 string / {text} / [{text}]。
pub fn extract_sampling_text(result: &Value) -> Option<String> {
    let content = &result["content"];
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(items) = content.as_array() {
        for item in items {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
        }
    }
    result
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

const SAMPLING_ID_PREFIX: &str = "cb-sampling-";

/// sampling 的发送/接收状态：放在 Arc 里，Sampler 必须是 Send + Sync。
struct SamplerIo {
    to_io: std::sync::mpsc::Sender<String>,
    pending: std::sync::Mutex<std::collections::HashMap<String, std::sync::mpsc::Sender<String>>>,
    next_id: std::sync::atomic::AtomicU64,
    supported: std::sync::atomic::AtomicBool,
}

impl SamplerIo {
    fn ask_blocking(&self, prompt: &str, max_tokens: u64) -> Result<String, CbError> {
        use std::sync::atomic::Ordering;
        if !self.supported.load(Ordering::SeqCst) {
            return Err(CbError::new_local(
                "sampling_unavailable",
                "client did not advertise the sampling capability",
                "MCP client must implement sampling/createMessage",
            ));
        }
        let id = format!(
            "{SAMPLING_ID_PREFIX}{}",
            self.next_id.fetch_add(1, Ordering::SeqCst) + 1
        );
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        self.pending
            .lock()
            .map_err(|_| CbError::other("sampling lock poisoned"))?
            .insert(id.clone(), tx);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "sampling/createMessage",
            "params": {
                "messages": [{
                    "role": "user",
                    "content": { "type": "text", "text": prompt }
                }],
                "maxTokens": max_tokens,
            }
        });
        if self.to_io.send(request.to_string()).is_err() {
            return Err(CbError::other("stdio writer is gone"));
        }
        let line = match rx.recv_timeout(std::time::Duration::from_secs(300)) {
            Ok(line) => line,
            Err(_) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                return Err(CbError::timeout(
                    "timed out after 300s waiting for the client's sampling response",
                ));
            }
        };
        let value: Value = serde_json::from_str(&line)
            .map_err(|e| CbError::other(format!("client sampling response is not json: {e}")))?;
        if let Some(error) = value.get("error") {
            return Err(CbError::other(format!("client sampling error: {error}")));
        }
        extract_sampling_text(&value["result"])
            .ok_or_else(|| CbError::other("client sampling response has no text"))
    }
}

struct SamplingSampler {
    io: std::sync::Arc<SamplerIo>,
}

impl cb_core::ai::ModelSampler for SamplingSampler {
    fn ask<'a>(&'a self, prompt: &'a str) -> cb_core::ai::SamplerFuture<'a> {
        // 采样是阻塞的 stdio 往返：同步做完再返回 ready future，
        // 这样 future 是 Send，也不会把非 Send 的锁带进 async 块。
        Box::pin(std::future::ready(self.io.ask_blocking(prompt, 400)))
    }
}

/// stdio 读写放到两个线程：读线程负责把 sampling 响应路由回等待者，写线程独占 stdout。
fn spawn_io_threads(
    sampler: std::sync::Arc<SamplerIo>,
    to_io: std::sync::mpsc::Receiver<String>,
    from_io: std::sync::mpsc::Sender<String>,
) {
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(std::io::stdin().lock());
        loop {
            let mut buf: Vec<u8> = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            let line = String::from_utf8_lossy(&buf).to_string();
            let mut routed = false;
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    if id.starts_with(SAMPLING_ID_PREFIX) {
                        if let Ok(mut pending) = sampler.pending.lock() {
                            if let Some(tx) = pending.remove(id) {
                                let _ = tx.send(line.clone());
                                routed = true;
                            }
                        }
                    }
                }
            }
            if !routed && from_io.send(line).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let mut writer = std::io::stdout();
        while let Ok(payload) = to_io.recv() {
            let _ = writeln!(writer, "{payload}");
            let _ = writer.flush();
        }
    });
}

/// 阻塞在 stdin 上逐行处理 JSON-RPC 请求；EOF 退出。
pub async fn serve_stdio(ctx: &Ctx, replay: Option<PathBuf>) -> anyhow::Result<()> {
    let (to_io_tx, to_io_rx) = std::sync::mpsc::channel::<String>();
    let (from_io_tx, from_io_rx) = std::sync::mpsc::channel::<String>();
    let sampler_io = std::sync::Arc::new(SamplerIo {
        to_io: to_io_tx.clone(),
        pending: std::sync::Mutex::new(std::collections::HashMap::new()),
        next_id: std::sync::atomic::AtomicU64::new(0),
        supported: std::sync::atomic::AtomicBool::new(false),
    });
    spawn_io_threads(sampler_io.clone(), to_io_rx, from_io_tx);
    let sampler = std::sync::Arc::new(SamplingSampler {
        io: sampler_io.clone(),
    });
    let ctx = ctx.clone().with_sampler(sampler);
    let write = |payload: String| {
        let _ = to_io_tx.send(payload);
    };

    while let Ok(line) = from_io_rx.recv() {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = rpc_error(Value::Null, -32700, format!("parse error: {e}"));
                write(serde_json::to_string(&resp).unwrap_or_default());
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request["method"].as_str().unwrap_or_default().to_string();

        // 通知类消息没有 id，不回包。
        if id.is_none() {
            tracing::debug!(method = %method, "notification");
            continue;
        }
        let id = id.unwrap_or(Value::Null);

        if method == "initialize" {
            let supports = request["params"]["capabilities"]["sampling"].is_object();
            sampler_io
                .supported
                .store(supports, std::sync::atomic::Ordering::SeqCst);
        }

        let response = match method.as_str() {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "cb", "version": cb_core::version::VERSION },
                }
            }),
            "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": tool_list() }),
            "tools/call" => {
                let result = call_tool(&ctx, replay.as_deref(), &request["params"]).await;
                json!({ "jsonrpc": "2.0", "id": id, "result": result })
            }
            other => rpc_error(id, -32601, format!("method not found: {other}")),
        };
        write(serde_json::to_string(&response).unwrap_or_default());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_text_accepts_all_three_shapes() {
        assert_eq!(
            extract_sampling_text(&json!({"content": "hi"})),
            Some("hi".to_string())
        );
        assert_eq!(
            extract_sampling_text(&json!({"content": {"type": "text", "text": "hi"}})),
            Some("hi".to_string())
        );
        assert_eq!(
            extract_sampling_text(&json!({"content": [{"type": "text", "text": "hi"}]})),
            Some("hi".to_string())
        );
        assert_eq!(
            extract_sampling_text(&json!({"text": "hi"})),
            Some("hi".to_string())
        );
        assert_eq!(extract_sampling_text(&json!({"content": []})), None);
    }
}
