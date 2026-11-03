//! `cb session`: 和常驻 daemon 说话（句柄 / 快照 / 差量）/ talk to the daemon.

use cb_core::error::CbError;
use cb_core::Envelope;
use cb_session::{transport, Request, SnapshotKind, SnapshotView};
use serde_json::{json, Value};

#[derive(Debug, clap::Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub cmd: SessionCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum SessionCmd {
    /// 打开页面并分配句柄 · Open a page and assign a handle
    Open {
        /// 目标 URL · Target URL
        url: String,
        /// 跳过缓存 · Bypass the cache
        #[arg(long)]
        fresh: bool,
    },
    /// 取快照；带 --since 只回差量 · Get a snapshot (delta with --since)
    Get {
        /// 句柄（如 @1）· Handle such as @1
        handle: String,
        /// 从哪个 revision 之后开始要 · Revision you already have
        #[arg(long, value_name = "REV")]
        since: Option<u64>,
    },
    /// 重新抓取并生成新 revision · Refetch and bump the revision
    Refresh {
        /// 句柄 · Handle
        handle: String,
        /// 跳过缓存 · Bypass the cache
        #[arg(long)]
        fresh: bool,
    },
    /// 列出所有句柄 · List handles
    List,
    /// 关闭句柄 · Close a handle
    Close {
        /// 句柄 · Handle
        handle: String,
    },
    /// 探活 · Check that the daemon is alive
    Ping,
}

pub async fn run(
    args: &SessionArgs,
    pipe: &str,
    json: bool,
    started: std::time::Instant,
    profile: &str,
) -> Result<i32, CbError> {
    let (method, params) = match &args.cmd {
        SessionCmd::Open { url, fresh } => ("open", json!({ "url": url, "fresh": fresh })),
        SessionCmd::Get { handle, since } => ("get", json!({ "handle": handle, "since": since })),
        SessionCmd::Refresh { handle, fresh } => {
            ("refresh", json!({ "handle": handle, "fresh": fresh }))
        }
        SessionCmd::List => ("list", json!({})),
        SessionCmd::Close { handle } => ("close", json!({ "handle": handle })),
        SessionCmd::Ping => ("ping", json!({})),
    };
    let result = call(pipe, method, params).await?;
    let meta = crate::output::meta(started.elapsed().as_millis() as u64, false, profile);
    if json {
        crate::output::print_json(&Envelope::ok(meta, result));
        return Ok(0);
    }
    let text = match &args.cmd {
        SessionCmd::Open { .. } | SessionCmd::Get { .. } | SessionCmd::Refresh { .. } => {
            let view: SnapshotView = serde_json::from_value(result)
                .map_err(|e| CbError::other(format!("daemon returned a bad snapshot: {e}")))?;
            render_snapshot(&view)
        }
        SessionCmd::List => render_list(&result),
        SessionCmd::Close { .. } => {
            format!("closed {}", result["closed"].as_str().unwrap_or_default())
        }
        SessionCmd::Ping => format!(
            "pong  version {}  sessions {}",
            result["version"].as_str().unwrap_or("?"),
            result["sessions"].as_u64().unwrap_or(0)
        ),
    };
    println!("{text}");
    Ok(0)
}

async fn call(pipe: &str, method: &str, params: Value) -> Result<Value, CbError> {
    let response = transport::call(pipe, Request::new(method, params))
        .await
        .map_err(|err| {
            CbError::new_local(
                "daemon_unavailable",
                format!(
                    "cannot reach cb daemon on {}: {err}",
                    transport::endpoint_label(pipe)
                ),
                "start it with: cb daemon (optionally --replay FILE)",
            )
        })?;
    match response.error {
        Some(body) => Err(CbError { body }),
        None => Ok(response.result.unwrap_or(Value::Null)),
    }
}

fn render_snapshot(view: &SnapshotView) -> String {
    let kind = match view.kind {
        SnapshotKind::Full => "full",
        SnapshotKind::Delta => "delta",
        SnapshotKind::Unchanged => "unchanged",
    };
    let mut out = format!(
        "{}  {}  rev {}  [{}]",
        view.handle, view.title, view.revision, kind
    );
    out.push('\n');
    out.push_str(&view.url);
    out.push('\n');
    match view.kind {
        SnapshotKind::Full => {
            let text = view
                .blocks
                .as_ref()
                .map(|blocks| cb_session::blocks_to_text(blocks))
                .unwrap_or_default();
            out.push('\n');
            out.push_str(&text);
        }
        SnapshotKind::Delta => {
            out.push('\n');
            out.push_str(&cb_session::ops_to_text(&view.ops));
        }
        SnapshotKind::Unchanged => {
            out.push_str("(unchanged)");
        }
    }
    out
}

fn render_list(result: &Value) -> String {
    let Some(sessions) = result["sessions"].as_array() else {
        return "no sessions".to_string();
    };
    if sessions.is_empty() {
        return "no sessions".to_string();
    }
    sessions
        .iter()
        .map(|session| {
            format!(
                "{}  rev {}  {} block(s)  {}",
                session["handle"].as_str().unwrap_or("?"),
                session["revision"].as_u64().unwrap_or(0),
                session["blocks"].as_u64().unwrap_or(0),
                session["url"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}
