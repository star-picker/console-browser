//! cb: 命令行 + MCP 浏览器。M1 只实现 fetch / history / selftest 入口。

mod commands;
mod output;

use cb_core::fetch::Format;
use cb_core::{CbError, Ctx, Envelope};
use cb_net::Profile;
use cb_store::Store;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(
    name = "cb",
    version,
    about = "console-browser: a browser for AI and terminals"
)]
struct Cli {
    /// 数据库文件（默认 %LOCALAPPDATA%\console-browser\cb.db）
    #[arg(long, global = true, env = "CB_DB", value_name = "FILE")]
    db: Option<PathBuf>,

    /// 机器可读输出（stdout 只有 JSON）
    #[arg(long, global = true)]
    json: bool,

    /// 调试日志到 stderr
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    /// TLS 指纹 profile
    #[arg(long, global = true, default_value = "chrome", value_name = "P")]
    profile: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// 取一个页面并输出 markdown / html / text / json
    Fetch(commands::fetch::FetchArgs),
    /// 浏览历史
    History {
        #[arg(long, value_name = "Q")]
        query: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// 自检：八项机器可读检查
    Selftest,
    /// 列出能力表（CLI / MCP / agent 共用）
    Tools,
    /// MCP 服务端（给 agent 调用）
    Mcp(commands::mcp::McpArgs),
    /// 本地库维护（prune / stats）
    Db(commands::db::DbArgs),
}

fn default_db_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("console-browser").join("cb.db")
}

fn init_tracing(verbose: bool) {
    let level = if verbose { "debug" } else { "warn" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let started = Instant::now();

    let profile = match Profile::parse(&cli.profile) {
        Some(p) => p,
        None => {
            let err = CbError::usage(format!(
                "unknown profile: {} (chrome|firefox|safari)",
                cli.profile
            ));
            output::report_error(
                &err,
                cli.json,
                started.elapsed().as_millis() as u64,
                &cli.profile,
            );
            std::process::exit(err.exit_code());
        }
    };

    let db = cli.db.clone().unwrap_or_else(default_db_path);
    let store = match Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            let err = CbError::other(format!("open db {}: {e}", db.display()));
            output::report_error(
                &err,
                cli.json,
                started.elapsed().as_millis() as u64,
                &cli.profile,
            );
            std::process::exit(err.exit_code());
        }
    };
    let ctx = Ctx::new(store, profile);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = runtime.block_on(run(&cli, &ctx, started));
    match result {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            output::report_error(
                &err,
                cli.json,
                started.elapsed().as_millis() as u64,
                &cli.profile,
            );
            std::process::exit(err.exit_code());
        }
    }
}

async fn run(cli: &Cli, ctx: &Ctx, started: Instant) -> Result<i32, CbError> {
    match &cli.cmd {
        Cmd::Fetch(args) => {
            let opts = args.to_opts().map_err(|e| CbError::usage(e.to_string()))?;
            let outcome = cb_core::fetch_page(ctx, &opts).await?;
            let elapsed = started.elapsed().as_millis() as u64;
            if cli.json && args.out.is_some() {
                return Err(CbError::usage(
                    "--out and --json are mutually exclusive: pick a file or a JSON envelope",
                ));
            }
            let body = match opts.format {
                Format::Md => outcome.markdown.clone(),
                Format::Html => outcome.raw.clone(),
                Format::Text => cb_extract::to_plain_text(&outcome.markdown),
                Format::Json => serde_json::to_string_pretty(&outcome)
                    .map_err(|e| CbError::other(e.to_string()))?,
            };

            if cli.json {
                let mut data =
                    serde_json::to_value(&outcome).map_err(|e| CbError::other(e.to_string()))?;
                if let Some(obj) = data.as_object_mut() {
                    obj.remove("raw");
                    obj.remove("markdown");
                    obj.insert(
                        "content".to_string(),
                        serde_json::Value::String(body.clone()),
                    );
                }
                let envelope = Envelope::ok(
                    output::meta(elapsed, outcome.cached, ctx.profile.as_str()),
                    data,
                );
                output::print_json(&envelope);
            } else if let Some(path) = &args.out {
                std::fs::write(path, &body)
                    .map_err(|e| CbError::other(format!("write {}: {e}", path.display())))?;
            } else {
                print!("{body}");
                if !body.ends_with('\n') {
                    println!();
                }
            }
            Ok(0)
        }
        Cmd::History { query, limit } => {
            let rows = ctx
                .store
                .list_history(query.as_deref(), *limit)
                .map_err(cb_core::error::store_err)?;
            if cli.json {
                let envelope = Envelope::ok(
                    output::meta(
                        started.elapsed().as_millis() as u64,
                        false,
                        ctx.profile.as_str(),
                    ),
                    serde_json::json!({ "entries": rows }),
                );
                output::print_json(&envelope);
            } else if rows.is_empty() {
                println!("no history");
            } else {
                for row in rows {
                    println!("{}\t{}\t{}", row.ts, row.title, row.url);
                }
            }
            Ok(0)
        }
        Cmd::Mcp(args) => {
            match &args.cmd {
                commands::mcp::McpCmd::Serve { replay } => {
                    cb_mcp::serve_stdio(ctx, replay.clone())
                        .await
                        .map_err(|e| CbError::other(e.to_string()))?;
                }
            }
            Ok(0)
        }
        Cmd::Db(args) => {
            match &args.cmd {
                commands::db::DbCmd::Prune => {
                    let report = ctx
                        .store
                        .prune(cb_core::now_secs())
                        .map_err(cb_core::error::store_err)?;
                    ctx.store
                        .append_log(&cb_store::LogRow {
                            id: 0,
                            ts: cb_core::now_secs(),
                            level: "INFO".to_string(),
                            target: "cb.db".to_string(),
                            message: format!(
                                "prune: cache {} / history {} / logs {}",
                                report.cache_deleted, report.history_deleted, report.logs_deleted
                            ),
                            fields: None,
                        })
                        .map_err(cb_core::error::store_err)?;
                    if cli.json {
                        let envelope = Envelope::ok(
                            output::meta(
                                started.elapsed().as_millis() as u64,
                                false,
                                ctx.profile.as_str(),
                            ),
                            serde_json::to_value(&report)
                                .map_err(|e| CbError::other(e.to_string()))?,
                        );
                        output::print_json(&envelope);
                    } else {
                        println!(
                            "pruned: cache {} · history {} · logs {}",
                            report.cache_deleted, report.history_deleted, report.logs_deleted
                        );
                    }
                }
                commands::db::DbCmd::Stats => {
                    let counts = ctx.store.counts().map_err(cb_core::error::store_err)?;
                    if cli.json {
                        let envelope = Envelope::ok(
                            output::meta(
                                started.elapsed().as_millis() as u64,
                                false,
                                ctx.profile.as_str(),
                            ),
                            serde_json::to_value(&counts)
                                .map_err(|e| CbError::other(e.to_string()))?,
                        );
                        output::print_json(&envelope);
                    } else {
                        println!(
                            "pages {} · cache {} · history {} · logs {}",
                            counts.pages, counts.cache_entries, counts.history, counts.logs
                        );
                    }
                }
            }
            Ok(0)
        }
        Cmd::Selftest => {
            let report = cb_core::run_selftest(ctx);
            if cli.json {
                let data =
                    serde_json::to_value(&report).map_err(|e| CbError::other(e.to_string()))?;
                let envelope = Envelope::ok(
                    output::meta(
                        started.elapsed().as_millis() as u64,
                        false,
                        ctx.profile.as_str(),
                    ),
                    data,
                );
                output::print_json(&envelope);
            } else {
                for check in &report.checks {
                    println!("{:<6} {:<6} {}", check.status, check.name, check.detail);
                }
            }
            Ok(if report.all_ok { 0 } else { 1 })
        }
        Cmd::Tools => {
            let tools = cb_core::registry();
            if cli.json {
                let envelope = Envelope::ok(
                    output::meta(
                        started.elapsed().as_millis() as u64,
                        false,
                        ctx.profile.as_str(),
                    ),
                    serde_json::json!({ "tools": tools }),
                );
                output::print_json(&envelope);
            } else {
                for tool in tools {
                    println!("{:<16} [{}] {}", tool.name, tool.kind, tool.summary);
                }
            }
            Ok(0)
        }
    }
}
