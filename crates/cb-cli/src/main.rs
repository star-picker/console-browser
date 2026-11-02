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
    /// 自检（M1 占位，T6 补全 8 项检查）
    Selftest,
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
            let body = match opts.format {
                Format::Md => outcome.markdown.clone(),
                Format::Html => outcome.raw.clone(),
                Format::Text => outcome.markdown.clone(),
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
        Cmd::Selftest => {
            println!("selftest: not implemented yet (T6)");
            Ok(0)
        }
    }
}
