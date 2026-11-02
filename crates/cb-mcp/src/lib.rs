//! cb-mcp: 手写 JSON-RPC 2.0 over stdio 的 MCP 服务端。
//! 只实现需要的子集：initialize / tools/list / tools/call / ping。

use cb_core::fetch::Format;
use cb_core::{registry, Ctx, FetchOpts};
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
                "description": t.summary,
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

/// 阻塞在 stdin 上逐行处理 JSON-RPC 请求；EOF 退出。
pub async fn serve_stdio(ctx: &Ctx, replay: Option<PathBuf>) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();

    loop {
        // 逐字节读到换行：单行不是合法 UTF-8 也不能把服务端搞死。
        let mut buf: Vec<u8> = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
        let line = String::from_utf8_lossy(&buf).to_string();
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = rpc_error(Value::Null, -32700, format!("parse error: {e}"));
                writeln!(stdout, "{resp}")?;
                stdout.flush()?;
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

        let response = match method.as_str() {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "cb", "version": env!("CARGO_PKG_VERSION") },
                }
            }),
            "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": tool_list() }),
            "tools/call" => {
                let result = call_tool(ctx, replay.as_deref(), &request["params"]).await;
                json!({ "jsonrpc": "2.0", "id": id, "result": result })
            }
            other => rpc_error(id, -32601, format!("method not found: {other}")),
        };

        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}
