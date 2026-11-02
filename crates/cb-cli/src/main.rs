//! cb: 命令行 + MCP 浏览器 · console browser for AI and terminals.

mod commands;
mod output;

use cb_core::fetch::Format;
use cb_core::i18n::{self, Lang};
use cb_core::{CbError, Ctx, Envelope};
use cb_net::Profile;
use cb_store::Store;
use clap::{Parser, Subcommand};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(
    name = "cb",
    version = cb_core::version::VERSION,
    about = "console-browser · 网页转 AI 可读文本 / web pages to AI-readable text",
    long_about = "console-browser (cb)\n\
        把网页变成干净的 markdown，本地 sqlite 存一切，可以挂成 MCP 服务端给 agent 用。\n\
        Turns pages into clean markdown, stores everything in local sqlite, and serves MCP tools to agents.\n\n\
        不带参数运行 = 交互模式（双击 exe 也是这个）。No arguments = interactive mode (what double-clicking gets you).",
    after_help = "EXAMPLES / 例子:\n  cb fetch https://example.com/a\n  cb fetch https://example.com/a --json --max-tokens 800\n  cb export https://example.com/a --to html --out page.html\n  cb mcp serve\n  cb\n\nPer-command help / 单命令帮助:\n  cb fetch --help\n  cb help fetch"
)]
struct Cli {
    /// 数据库文件 · Database file (default: LOCALAPPDATA/console-browser/cb.db)
    #[arg(long, global = true, env = "CB_DB", value_name = "FILE")]
    db: Option<PathBuf>,

    /// 机器可读输出，stdout 只有 JSON · Machine-readable output, JSON only on stdout
    #[arg(long, global = true)]
    json: bool,

    /// 调试日志到 stderr · Debug logs on stderr
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    /// TLS 指纹 · TLS fingerprint profile: chrome | firefox | safari
    #[arg(long, global = true, default_value = "chrome", value_name = "P")]
    profile: String,

    /// 界面语言 · UI language: zh | en
    #[arg(long, global = true, value_name = "LANG")]
    lang: Option<String>,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// 取页面并输出 · Fetch a page and print it
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb fetch https://example.com/a\n  cb fetch https://example.com/a --to text --max-tokens 500\n  cb fetch https://example.com/a --json --replay case.cassette.json"
    )]
    Fetch(commands::fetch::FetchArgs),

    /// 导出单文件 HTML 或 markdown · Export as single-file HTML or markdown
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb export https://example.com/a --to html --out page.html\n  cb export https://example.com/a --to md"
    )]
    Export(commands::export::ExportArgs),

    /// 浏览历史 · Browse history
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb history --limit 10\n  cb --json history --query example.com"
    )]
    History {
        /// 关键词（URL 或标题）· Keyword in URL or title
        #[arg(long, value_name = "Q")]
        query: Option<String>,
        /// 条数 · How many rows
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },

    /// 能力表（CLI / MCP / agent 共用）· Tool registry shared by CLI, MCP and agents
    #[command(after_help = "EXAMPLES / 例子:\n  cb tools\n  cb --json tools")]
    Tools,

    /// 八项自检 · Eight machine-readable checks
    #[command(after_help = "EXAMPLES / 例子:\n  cb selftest\n  cb --json selftest")]
    Selftest,

    /// MCP 服务端 · MCP server
    Mcp(commands::mcp::McpArgs),

    /// 结构化搜索 · Structured web search (ddg / bing / gh / arxiv)
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb search \"rust http client\" --engine ddg\n  cb search ripgrep --engine gh --limit 5 --json\n  cb search diffusion --engine arxiv"
    )]
    Search(commands::search::SearchArgs),

    /// AI 配置 / 用量 / 提问 · AI config, usage ledger, one-shot ask
    Ai(commands::ai::AiArgs),

    /// 自主循环 · Autonomous loop (model picks actions, browser executes)
    Agent(commands::agent::AgentArgs),

    /// 反爬挑战统计 · Anti-bot challenge stats
    Challenge(commands::challenge::ChallengeArgs),

    /// 人工接力登录 · Human-in-the-loop login (browser window or paste cookies)
    Login(commands::login::LoginArgs),

    /// 纯文本输出（管道友好）· Plain text output for pipes
    Plain {
        /// 目标 URL · Target URL
        url: String,
        /// 从 cassette 回放 · Replay a cassette
        #[arg(long, value_name = "FILE")]
        replay: Option<PathBuf>,
    },

    /// 登录态 cookie · Session cookies (names only, never values)
    Cookies(commands::cookies::CookiesArgs),

    /// 本地库维护 · Local database maintenance
    Db(commands::db::DbArgs),

    /// 全屏浏览器界面 · Full-screen browser UI
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb tui\n  cb tui --url example.com\n  cb tui --no-mouse"
    )]
    Tui(commands::tui::TuiArgs),

    /// 交互模式（不带参数运行也可）· Interactive shell (also what plain cb does)
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb shell\n  cb\n\nIn the shell / 交互模式里:\n  help            list commands\n  lang en         switch language\n  fetch <url>     run any command without restarting\n  exit"
    )]
    Shell,
}

fn default_db_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("console-browser").join("cb.db")
}

fn history_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("console-browser").join("shell_history.txt")
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

/// 极简分词：支持双引号与单引号，够交互模式用。
fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn globals_argv(cli: &Cli) -> Vec<String> {
    let mut v = vec!["cb".to_string()];
    if let Some(db) = &cli.db {
        v.push("--db".to_string());
        v.push(db.display().to_string());
    }
    if cli.json {
        v.push("--json".to_string());
    }
    if cli.verbose {
        v.push("-v".to_string());
    }
    v.push("--profile".to_string());
    v.push(cli.profile.clone());
    v.push("--lang".to_string());
    v.push(i18n::current().as_str().to_string());
    v
}

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let db_path = cli.db.clone().unwrap_or_else(default_db_path);
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            let err = CbError::other(format!("open db {}: {e}", db_path.display()));
            output::report_error(&err, cli.json, 0, &cli.profile);
            std::process::exit(err.exit_code());
        }
    };

    // 语言优先级：--lang > CB_LANG > 库里的设置 > 系统区域。
    let stored = store.get_setting("lang").ok().flatten();
    let explicit = cli.lang.clone().or(stored);
    i18n::set(i18n::detect(explicit.as_deref()));

    let profile = match Profile::parse(&cli.profile) {
        Some(p) => p,
        None => {
            let err = CbError::usage(format!(
                "unknown profile: {} (chrome|firefox|safari)",
                cli.profile
            ));
            output::report_error(&err, cli.json, 0, &cli.profile);
            std::process::exit(err.exit_code());
        }
    };

    let ctx = Ctx::new(store, profile);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    let is_tty = std::io::stdout().is_terminal() && std::io::stdin().is_terminal();
    match cli.cmd {
        None => {
            if is_tty {
                let opts = cb_tui::DriverOpts {
                    replay: None,
                    mouse: true,
                    start_url: None,
                    no_restore: false,
                    ascii: std::env::var("CB_ASCII").is_ok(),
                    no_color: std::env::var("NO_COLOR").is_ok(),
                };
                match cb_tui::run_tui(ctx, opts) {
                    Ok(code) => std::process::exit(code),
                    Err(e) => {
                        eprintln!("cb: tui failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
            let code = run_shell(&runtime, &cli, &ctx);
            std::process::exit(code);
        }
        Some(Cmd::Shell) => {
            let code = run_shell(&runtime, &cli, &ctx);
            std::process::exit(code);
        }
        Some(Cmd::Tui(args)) => {
            if !is_tty {
                let err =
                    CbError::usage("cb tui needs a terminal (use cb fetch / cb shell when piping)");
                output::report_error(&err, cli.json, 0, ctx.profile.as_str());
                std::process::exit(err.exit_code());
            }
            let opts = cb_tui::DriverOpts {
                replay: args.replay.clone(),
                mouse: !args.no_mouse,
                start_url: args.url.clone(),
                no_restore: args.no_restore,
                ascii: args.ascii || std::env::var("CB_ASCII").is_ok(),
                no_color: args.no_color || std::env::var("NO_COLOR").is_ok(),
            };
            match cb_tui::run_tui(ctx, opts) {
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("cb: tui failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(ref cmd) => {
            let started = Instant::now();
            match runtime.block_on(run(&cli, &ctx, started, cmd)) {
                Ok(code) => std::process::exit(code),
                Err(err) => {
                    output::report_error(
                        &err,
                        cli.json,
                        started.elapsed().as_millis() as u64,
                        ctx.profile.as_str(),
                    );
                    std::process::exit(err.exit_code());
                }
            }
        }
    }
}

fn run_shell(runtime: &tokio::runtime::Runtime, base: &Cli, ctx: &Ctx) -> i32 {
    println!("{}", i18n::t("shell.banner"));
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("cb: {e}");
            return 1;
        }
    };
    let hist = history_path();
    let _ = rl.load_history(&hist);

    loop {
        match rl.readline(i18n::t("shell.prompt")) {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(&line);
                let parts = split_args(&line);
                if parts.is_empty() {
                    continue;
                }
                let head = parts[0].to_ascii_lowercase();
                match head.as_str() {
                    "exit" | "quit" | "q" => break,
                    "help" | "?" => {
                        println!("{}", i18n::t("shell.help"));
                        continue;
                    }
                    "clear" | "cls" => {
                        print!("\x1b[2J\x1b[H");
                        continue;
                    }
                    "lang" => {
                        let target = parts.get(1).map(|s| s.as_str());
                        match target.and_then(Lang::parse) {
                            Some(lang) => {
                                i18n::set(lang);
                                let _ = ctx.store.set_setting("lang", lang.as_str());
                                println!("{}{}", i18n::t("shell.lang"), lang.as_str());
                            }
                            None => println!("usage / 用法: lang zh | lang en"),
                        }
                        continue;
                    }
                    _ => {}
                }
                let mut argv = globals_argv(base);
                argv.extend(parts);
                match Cli::try_parse_from(&argv) {
                    Ok(parsed) => {
                        let started = Instant::now();
                        match parsed.cmd {
                            Some(ref cmd) => {
                                if let Err(err) = runtime.block_on(run(&parsed, ctx, started, cmd))
                                {
                                    output::report_error(
                                        &err,
                                        parsed.json,
                                        started.elapsed().as_millis() as u64,
                                        ctx.profile.as_str(),
                                    );
                                }
                            }
                            None => println!("{}", i18n::t("shell.help")),
                        }
                    }
                    Err(e) => {
                        // --help / --version 也走这里：打印后继续，不退出交互模式。
                        print!("{e}");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("cb: {e}");
                break;
            }
        }
    }
    let _ = rl.save_history(&hist);
    println!("{}", i18n::t("shell.goodbye"));
    0
}

async fn run(cli: &Cli, ctx: &Ctx, started: Instant, cmd: &Cmd) -> Result<i32, CbError> {
    match cmd {
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
        Cmd::Export(args) => {
            let format = args.to.to_ascii_lowercase();
            if format != "html" && format != "md" && format != "markdown" {
                return Err(CbError::usage(format!(
                    "unknown --to value: {} (html|md)",
                    args.to
                )));
            }
            let opts = cb_core::FetchOpts {
                url: args.url.clone(),
                format: cb_core::Format::Html,
                fresh: args.fresh,
                replay: args.replay.clone(),
                ..Default::default()
            };
            let outcome = cb_core::fetch_page(ctx, &opts).await?;
            let content = if format == "html" {
                cb_core::export::single_file_html(&outcome.raw, &outcome.title, &outcome.final_url)
            } else {
                outcome.markdown.clone()
            };
            let elapsed = started.elapsed().as_millis() as u64;
            let meta = output::meta(elapsed, outcome.cached, ctx.profile.as_str());
            if let Some(path) = &args.out {
                std::fs::write(path, &content)
                    .map_err(|e| CbError::other(format!("write {}: {e}", path.display())))?;
                if !cli.json {
                    eprintln!(
                        "{} {} ({} {})",
                        i18n::t("common.wrote"),
                        path.display(),
                        content.len(),
                        i18n::t("common.bytes")
                    );
                }
            }
            if cli.json {
                let mut data = serde_json::json!({
                    "url": outcome.url,
                    "final_url": outcome.final_url,
                    "format": format,
                    "bytes": content.len(),
                });
                if args.out.is_none() {
                    data["content"] = serde_json::Value::String(content);
                }
                let envelope = Envelope::ok(meta, data);
                output::print_json(&envelope);
            } else if args.out.is_none() {
                print!("{content}");
                if !content.ends_with('\n') {
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
        Cmd::Search(args) => {
            let engine = cb_core::Engine::parse(&args.engine).ok_or_else(|| {
                CbError::usage(format!(
                    "unknown engine: {} (ddg|bing|gh|arxiv)",
                    args.engine
                ))
            })?;
            let mut opts = cb_core::SearchOpts::new(engine, args.query.clone());
            opts.limit = args.limit;
            opts.replay = args.replay.clone();
            opts.timeout_ms = args.timeout_ms;
            let results = cb_core::search(ctx, &opts).await?;
            let elapsed = started.elapsed().as_millis() as u64;
            let meta = output::meta(elapsed, false, ctx.profile.as_str());
            if cli.json {
                let envelope = Envelope::ok(
                    meta,
                    serde_json::json!({ "engine": engine.as_str(), "query": args.query, "results": results }),
                );
                output::print_json(&envelope);
            } else if results.is_empty() {
                println!("no results");
            } else {
                for (i, r) in results.iter().enumerate() {
                    println!("{}. {}\n   {}\n   {}", i + 1, r.title, r.url, r.snippet);
                }
            }
            Ok(0)
        }
        Cmd::Ai(args) => match &args.cmd {
            commands::ai::AiCmd::Config => {
                let config = cb_core::AiConfig::from_env();
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        config.describe(),
                    );
                    output::print_json(&envelope);
                } else {
                    let described = config.describe();
                    println!(
                        "provider : {}",
                        described["provider"].as_str().unwrap_or("")
                    );
                    println!("model    : {}", described["model"].as_str().unwrap_or(""));
                    println!(
                        "base_url : {}",
                        described["base_url"].as_str().unwrap_or("")
                    );
                    println!(
                        "api_key  : {}",
                        if config.api_key.is_some() {
                            "present"
                        } else {
                            "missing"
                        }
                    );
                    println!(
                        "budget   : {}",
                        config
                            .daily_budget
                            .map(|b| b.to_string())
                            .unwrap_or_else(|| "unlimited".to_string())
                    );
                    let used = ctx
                        .store
                        .ai_tokens_used(cb_core::ai::today())
                        .map_err(cb_core::error::store_err)?;
                    println!("today    : {used} tokens");
                }
                Ok(0)
            }
            commands::ai::AiCmd::Usage { days } => {
                let rows = ctx
                    .store
                    .llm_usage(*days)
                    .map_err(cb_core::error::store_err)?;
                let today_used = ctx
                    .store
                    .ai_tokens_used(cb_core::ai::today())
                    .map_err(cb_core::error::store_err)?;
                if cli.json {
                    let entries: Vec<serde_json::Value> = rows
                        .iter()
                        .map(|(day, prompt, completion)| {
                            serde_json::json!({
                                "day": day,
                                "prompt_tokens": prompt,
                                "completion_tokens": completion,
                                "total": prompt + completion,
                            })
                        })
                        .collect();
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::json!({ "today_tokens": today_used, "days": entries }),
                    );
                    output::print_json(&envelope);
                } else if rows.is_empty() {
                    println!("no model calls recorded yet");
                } else {
                    for (day, prompt, completion) in rows {
                        println!("{day}	prompt {prompt}	completion {completion}");
                    }
                    println!("today: {today_used} tokens");
                }
                Ok(0)
            }
            commands::ai::AiCmd::Ask { prompt, max_tokens } => {
                let completion = cb_core::ai::complete(ctx, "ask", prompt, *max_tokens).await?;
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::to_value(&completion)
                            .map_err(|e| CbError::other(e.to_string()))?,
                    );
                    output::print_json(&envelope);
                } else {
                    println!("{}", completion.text);
                    eprintln!(
                        "-- {} tokens in / {} out · {} ms · {}",
                        completion.prompt_tokens,
                        completion.completion_tokens,
                        completion.latency_ms,
                        completion.model
                    );
                }
                Ok(0)
            }
        },
        Cmd::Agent(args) => match &args.cmd {
            commands::agent::AgentCmd::Run {
                goal,
                max_steps,
                budget,
                observation_chars,
                replay,
            } => {
                let mut opts = cb_core::AgentOpts::new(goal.clone());
                opts.max_steps = *max_steps;
                opts.max_tokens = *budget;
                opts.observation_chars = *observation_chars;
                opts.replay = replay.clone();
                let run = cb_core::run_agent(ctx, &opts).await?;
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::to_value(&run).map_err(|e| CbError::other(e.to_string()))?,
                    );
                    output::print_json(&envelope);
                } else {
                    println!("{}", run.answer);
                    eprintln!(
                        "-- {} steps · {} completion tokens",
                        run.steps.len(),
                        run.completion_tokens
                    );
                }
                Ok(0)
            }
        },
        Cmd::Login(args) => {
            let url = args.url.clone();
            if args.manual {
                use std::io::Read;
                let mut raw = String::new();
                std::io::stdin()
                    .read_to_string(&mut raw)
                    .map_err(|e| CbError::other(format!("read stdin: {e}")))?;
                let cookies = cb_assist::import_manual_cookies(&raw, &url);
                if cookies.is_empty() {
                    return Err(CbError::usage("no cookie pairs found on stdin"));
                }
                ctx.cookies.save(&cookies);
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::json!({ "saved": cookies.len(), "mode": "manual" }),
                    );
                    output::print_json(&envelope);
                } else {
                    println!("saved {} cookies for {url}", cookies.len());
                }
                return Ok(0);
            }
            let mut opts = cb_assist::AssistOpts::new(url.clone());
            opts.browser = args
                .browser
                .as_deref()
                .and_then(cb_assist::BrowserKind::parse);
            opts.keep_open = args.keep_open;
            opts.timeout = std::time::Duration::from_secs(args.timeout_secs);
            let cookies = cb_assist::run_assist(&opts, |event| match event {
                cb_assist::AssistEvent::Started { browser, port } => {
                    eprintln!("cb: opened {} on debug port {port}", browser.as_str());
                    eprintln!(
                        "cb: finish the login in that window; cookies are collected automatically"
                    );
                }
                cb_assist::AssistEvent::CookiesUpdated { count } => {
                    eprintln!("cb: cookies so far: {count}");
                }
                _ => {}
            })
            .await
            .map_err(|e| CbError::other(format!("assist failed: {e}")))?;
            if cookies.is_empty() {
                return Err(CbError::other("no cookies collected (login cancelled?)"));
            }
            ctx.cookies.save(&cookies);
            if cli.json {
                let envelope = Envelope::ok(
                    output::meta(
                        started.elapsed().as_millis() as u64,
                        false,
                        ctx.profile.as_str(),
                    ),
                    serde_json::json!({ "saved": cookies.len(), "mode": "browser" }),
                );
                output::print_json(&envelope);
            } else {
                println!("saved {} cookies for {url}", cookies.len());
                println!("reload the page (r) to use the session");
            }
            Ok(0)
        }
        Cmd::Plain { url, replay } => {
            let opts = cb_core::FetchOpts {
                url: url.clone(),
                format: cb_core::Format::Text,
                replay: replay.clone(),
                ..Default::default()
            };
            let outcome = cb_core::fetch_page(ctx, &opts).await?;
            let text = cb_extract::to_plain_text(&outcome.markdown);
            if cli.json {
                let envelope = Envelope::ok(
                    output::meta(
                        started.elapsed().as_millis() as u64,
                        outcome.cached,
                        ctx.profile.as_str(),
                    ),
                    serde_json::json!({ "url": outcome.final_url, "content": text }),
                );
                output::print_json(&envelope);
            } else {
                print!("{text}");
            }
            Ok(0)
        }
        Cmd::Cookies(args) => match &args.cmd {
            commands::cookies::CookiesCmd::List { domain } => {
                let entries: Vec<serde_json::Value> = match domain {
                    Some(host) => ctx
                        .cookies
                        .load(host)
                        .into_iter()
                        .map(cookie_json)
                        .collect(),
                    None => {
                        let domains = ctx
                            .store
                            .cookie_domains()
                            .map_err(cb_core::error::store_err)?;
                        let mut all = Vec::new();
                        for host in domains {
                            for c in ctx.cookies.load(&host) {
                                all.push(cookie_json(c));
                            }
                        }
                        all
                    }
                };
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::json!({ "entries": entries, "count": entries.len() }),
                    );
                    output::print_json(&envelope);
                } else if entries.is_empty() {
                    println!("no cookies stored");
                } else {
                    for e in &entries {
                        println!(
                            "{}  {}  {}",
                            e["domain"].as_str().unwrap_or(""),
                            e["name"].as_str().unwrap_or(""),
                            e["path"].as_str().unwrap_or("")
                        );
                    }
                }
                Ok(0)
            }
            commands::cookies::CookiesCmd::Clear { domain } => {
                ctx.cookies.clear(domain.as_deref());
                if cli.json {
                    let envelope = Envelope::ok(
                        output::meta(
                            started.elapsed().as_millis() as u64,
                            false,
                            ctx.profile.as_str(),
                        ),
                        serde_json::json!({ "cleared": true, "domain": domain }),
                    );
                    output::print_json(&envelope);
                } else {
                    println!(
                        "cleared cookies{}",
                        domain
                            .as_deref()
                            .map(|d| format!(" for {d}"))
                            .unwrap_or_default()
                    );
                }
                Ok(0)
            }
        },
        Cmd::Challenge(args) => match &args.cmd {
            commands::challenge::ChallengeCmd::Stats { days } => {
                let since = cb_core::now_secs() - days * 86_400;
                let rows = ctx
                    .store
                    .challenge_summary(since)
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
                    println!("no challenge events recorded");
                } else {
                    for row in rows {
                        println!(
                            "{:<28} {:<28} wins {} losses {}",
                            row.domain, row.strategy, row.wins, row.losses
                        );
                    }
                }
                Ok(0)
            }
        },
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
                            "{}: cache {} / history {} / logs {}",
                            i18n::t("db.pruned"),
                            report.cache_deleted,
                            report.history_deleted,
                            report.logs_deleted
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
                commands::db::DbCmd::Schema => {
                    let tables = ctx
                        .store
                        .schema_summary()
                        .map_err(cb_core::error::store_err)?;
                    if cli.json {
                        let envelope = Envelope::ok(
                            output::meta(
                                started.elapsed().as_millis() as u64,
                                false,
                                ctx.profile.as_str(),
                            ),
                            serde_json::json!({ "tables": tables }),
                        );
                        output::print_json(&envelope);
                    } else {
                        for table in tables {
                            println!("{} ({} rows)", table.name, table.rows);
                            for column in table.columns {
                                println!("    {column}");
                            }
                        }
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
                println!(
                    "{}",
                    if report.all_ok {
                        i18n::t("selftest.all_ok")
                    } else {
                        i18n::t("selftest.failed")
                    }
                );
            }
            Ok(if report.all_ok { 0 } else { 1 })
        }
        Cmd::Shell | Cmd::Tui(_) => Ok(0),
    }
}
fn cookie_json(c: cb_net::cookies::Cookie) -> serde_json::Value {
    serde_json::json!({
        "domain": c.domain,
        "name": c.name,
        "path": c.path,
        "secure": c.secure,
        "http_only": c.http_only,
        "expires_at": c.expires_at,
        "session_like": c.looks_like_session(),
    })
}
