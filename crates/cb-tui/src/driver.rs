//! 驱动层：终端事件循环 + 后台加载线程。
//!
//! 网络在独立线程里跑（自己的 tokio runtime），主线程只画屏与收事件，
//! 所以加载时界面不冻结、Esc 取消也能立刻生效。

use crate::app::{App, Effect, PageView};
use crate::keymap::key_to_action;
use crate::mode::Mode;
use crate::screen::Screen;
use cb_core::Ctx;
use cb_extract::{Block, LinkRef, NoticeLevel, RenderDoc, Span};
use crossterm::event::{self, Event as CtEvent, KeyCode, KeyEventKind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct DriverOpts {
    pub replay: Option<PathBuf>,
    pub mouse: bool,
    pub start_url: Option<String>,
    /// 不恢复上次标签页。
    pub no_restore: bool,
}

enum Req {
    Load {
        tab: usize,
        gen: u64,
        url: String,
    },
    Search {
        tab: usize,
        gen: u64,
        engine: String,
        query: String,
    },
    Export {
        url: String,
        html: bool,
    },
    SaveCookies {
        raw: String,
        url: String,
    },
    History,
    RunCommand {
        tab: usize,
        line: String,
    },
    AskAi {
        question: String,
        context: String,
    },
    Assist {
        url: String,
    },
}

enum UiEvent {
    Loaded {
        tab: usize,
        gen: u64,
        url: String,
        final_url: String,
        title: String,
        doc: RenderDoc,
        cached: bool,
        tokens: usize,
        raw: String,
        markdown: String,
    },
    NeedsHuman {
        tab: usize,
        gen: u64,
        url: String,
        kind: String,
        hints: Vec<String>,
    },
    Failed {
        tab: usize,
        gen: u64,
        code: String,
        message: String,
    },
    History(Vec<(i64, String, String)>),
    AiAnswer {
        question: String,
        answer: String,
    },
    CommandOutput {
        tab: usize,
        line: String,
        output: String,
    },
    Status(String),
}

pub fn run(ctx: Ctx, opts: DriverOpts) -> anyhow::Result<i32> {
    let mut screen = Screen::enter(opts.mouse)?;
    let (width, height) = screen.size();
    let mut app = App::new(width, height);

    let main_ctx = ctx.clone();
    let (req_tx, req_rx) = mpsc::channel::<Req>();
    let (ev_tx, ev_rx) = mpsc::channel::<UiEvent>();
    let replay = opts.replay.clone();
    let loader = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ev_tx.send(UiEvent::Status(format!("runtime error: {e}")));
                return;
            }
        };
        while let Ok(req) = req_rx.recv() {
            handle_req(&runtime, &ctx, replay.as_deref(), req, &ev_tx);
        }
    });

    if let Some(url) = opts.start_url.clone() {
        let effect = app.navigate(&url);
        dispatch(effect, &req_tx, &mut app);
    } else if !opts.no_restore {
        if let Some(snapshots) = load_session(&main_ctx) {
            app.restore_tabs(&snapshots);
            let url = app.tab().url().to_string();
            if !url.is_empty() {
                let effect = app.navigate(&url);
                dispatch(effect, &req_tx, &mut app);
            }
        }
    }

    loop {
        screen.draw(&app)?;

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                CtEvent::Key(key) if key.kind != KeyEventKind::Release => {
                    // 输入模式下普通字符直接进编辑缓冲
                    if matches!(app.mode, Mode::Input(_)) && key.modifiers.is_empty() {
                        if let KeyCode::Char(ch) = key.code {
                            app.omnibox.insert(ch);
                            continue;
                        }
                    }
                    if let Some(action) = key_to_action(app.mode, key) {
                        if let Some(effect) = app.apply(action) {
                            dispatch(effect, &req_tx, &mut app);
                        }
                    }
                }
                CtEvent::Resize(w, h) => app.resize(w, h),
                _ => {}
            }
        }

        while let Ok(ev) = ev_rx.try_recv() {
            apply_event(&mut app, ev);
        }
        if app.should_quit {
            break;
        }
    }

    save_session(&main_ctx, &app);
    drop(req_tx);
    let _ = loader.join();
    Ok(0)
}

fn dispatch(effect: Effect, tx: &mpsc::Sender<Req>, app: &mut App) {
    match effect {
        Effect::LoadPage { tab, url, gen } => {
            let _ = tx.send(Req::Load { tab, gen, url });
        }
        Effect::RunSearch {
            tab,
            engine,
            query,
            gen,
        } => {
            let _ = tx.send(Req::Search {
                tab,
                gen,
                engine,
                query,
            });
        }
        Effect::ExportMarkdown { url } => {
            let _ = tx.send(Req::Export { url, html: false });
        }
        Effect::ExportHtml { url } => {
            let _ = tx.send(Req::Export { url, html: true });
        }
        Effect::SaveCookies { raw } => {
            let url = app.tab().url().to_string();
            let _ = tx.send(Req::SaveCookies { raw, url });
        }
        Effect::OpenAssist { url } => {
            let _ = tx.send(Req::Assist { url });
        }
        Effect::LoadHistory => {
            let _ = tx.send(Req::History);
        }
        Effect::AskAi { question, context } => {
            let _ = tx.send(Req::AskAi { question, context });
        }
        Effect::RunPaletteAction(name) => {
            if let Some(action) = crate::palette::action_for(&name) {
                if let Some(next) = app.apply(action) {
                    dispatch(next, tx, app);
                }
            } else {
                app.set_status(format!("palette: unknown action {name}"));
            }
        }
        Effect::RunCommand { tab, line } => {
            let _ = tx.send(Req::RunCommand { tab, line });
        }
        Effect::CopyUrl { url } => {
            app.set_status(format!("url: {url}"));
        }
    }
}

fn apply_event(app: &mut App, ev: UiEvent) {
    match ev {
        UiEvent::Loaded {
            tab,
            gen,
            url,
            final_url,
            title,
            doc,
            cached,
            tokens,
            raw,
            markdown,
        } => {
            let view = PageView::ready(
                url, final_url, title, doc, app.width, cached, tokens, raw, markdown,
            );
            app.on_loaded(tab, gen, view);
        }
        UiEvent::NeedsHuman {
            tab,
            gen,
            url,
            kind,
            hints,
        } => {
            app.on_needs_human(tab, gen, &url, &kind, &hints);
        }
        UiEvent::Failed {
            tab,
            gen,
            code,
            message,
        } => {
            app.on_failed(tab, gen, &code, &message);
        }
        UiEvent::History(entries) => app.open_history_overlay(&entries),
        UiEvent::AiAnswer { question, answer } => app.set_ai_answer(&question, &answer),
        UiEvent::CommandOutput {
            tab: _,
            line,
            output,
        } => {
            let head: String = output.chars().take(400).collect();
            app.set_status(format!("{line} → {head}"));
        }
        UiEvent::Status(msg) => app.set_status(msg),
    }
}

fn handle_req(
    runtime: &tokio::runtime::Runtime,
    ctx: &Ctx,
    replay: Option<&std::path::Path>,
    req: Req,
    tx: &mpsc::Sender<UiEvent>,
) {
    match req {
        Req::Load { tab, gen, url } => {
            let opts = cb_core::FetchOpts {
                url: url.clone(),
                replay: replay.map(|p| p.to_path_buf()),
                ..Default::default()
            };
            match runtime.block_on(cb_core::fetch_page(ctx, &opts)) {
                Ok(outcome) => {
                    if let Some(wall) = &outcome.login {
                        let _ = tx.send(UiEvent::NeedsHuman {
                            tab,
                            gen,
                            url: outcome.final_url.clone(),
                            kind: wall.kind.as_str().to_string(),
                            hints: wall.hints.clone(),
                        });
                        return;
                    }
                    let doc = outcome.doc.clone().unwrap_or_else(|| {
                        cb_extract::to_document(&outcome.raw, &cb_extract::ExtractOpts::default())
                            .unwrap_or_default()
                    });
                    let _ = tx.send(UiEvent::Loaded {
                        tab,
                        gen,
                        url: outcome.url.clone(),
                        final_url: outcome.final_url.clone(),
                        title: outcome.title.clone(),
                        doc,
                        cached: outcome.cached,
                        tokens: outcome.tokens_estimate,
                        raw: outcome.raw.clone(),
                        markdown: outcome.markdown.clone(),
                    });
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed {
                        tab,
                        gen,
                        code: err.body.code.clone(),
                        message: err.body.message.clone(),
                    });
                }
            }
        }
        Req::Search {
            tab,
            gen,
            engine,
            query,
        } => {
            let parsed = cb_core::Engine::parse(&engine).unwrap_or(cb_core::Engine::DuckDuckGo);
            let mut sopts = cb_core::SearchOpts::new(parsed, query.clone());
            sopts.replay = replay.map(|p| p.to_path_buf());
            match runtime.block_on(cb_core::search(ctx, &sopts)) {
                Ok(results) => {
                    let doc = search_doc(&query, &results);
                    let _ = tx.send(UiEvent::Loaded {
                        tab,
                        gen,
                        url: format!("search://{engine}?{query}"),
                        final_url: format!("search://{engine}?{query}"),
                        title: format!("搜索 / search: {query}"),
                        doc,
                        cached: false,
                        tokens: 0,
                        raw: String::new(),
                        markdown: String::new(),
                    });
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed {
                        tab,
                        gen,
                        code: err.body.code.clone(),
                        message: err.body.message.clone(),
                    });
                }
            }
        }
        Req::Export { url, html } => {
            let opts = cb_core::FetchOpts {
                url: url.clone(),
                format: cb_core::Format::Html,
                replay: replay.map(|p| p.to_path_buf()),
                ..Default::default()
            };
            match runtime.block_on(cb_core::fetch_page(ctx, &opts)) {
                Ok(outcome) => {
                    let (content, ext) = if html {
                        (
                            cb_core::export::single_file_html(
                                &outcome.raw,
                                &outcome.title,
                                &outcome.final_url,
                            ),
                            "html",
                        )
                    } else {
                        (outcome.markdown.clone(), "md")
                    };
                    let base = if outcome.title.trim().is_empty() {
                        "page"
                    } else {
                        outcome.title.trim()
                    };
                    let safe: String = base
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || c == '-' || c == '_' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .take(60)
                        .collect();
                    let path = std::path::PathBuf::from(format!("{safe}.{ext}"));
                    match std::fs::write(&path, content) {
                        Ok(_) => {
                            let _ =
                                tx.send(UiEvent::Status(format!("exported {}", path.display())));
                        }
                        Err(e) => {
                            let _ = tx.send(UiEvent::Status(format!("export failed: {e}")));
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Status(format!("export failed: {}", err.body.code)));
                }
            }
        }
        Req::SaveCookies { raw, url } => {
            let host = url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
                .unwrap_or_default();
            if host.is_empty() {
                let _ = tx.send(UiEvent::Status("cookie import: no url".to_string()));
                return;
            }
            let now = cb_core::now_secs();
            let mut saved = 0usize;
            for part in raw.split(';') {
                if let Some(cookie) = cb_net::cookies::parse_set_cookie(part.trim(), &host, now) {
                    ctx.cookies.save(&[cookie]);
                    saved += 1;
                }
            }
            let _ = tx.send(UiEvent::Status(format!("cookies imported: {saved}")));
        }
        Req::History => {
            let rows = ctx.store.list_history(None, 100).unwrap_or_default();
            let entries: Vec<(i64, String, String)> =
                rows.into_iter().map(|r| (r.ts, r.title, r.url)).collect();
            let _ = tx.send(UiEvent::History(entries));
        }
        Req::RunCommand { tab, line } => {
            let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cb"));
            let output = std::process::Command::new(exe)
                .arg("--db")
                .arg("tui-subprocess")
                .args(line.split_whitespace())
                .output();
            let text = match output {
                Ok(out) => {
                    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                    if text.is_empty() {
                        text = String::from_utf8_lossy(&out.stderr).to_string();
                    }
                    text
                }
                Err(e) => format!("command failed: {e}"),
            };
            let _ = tx.send(UiEvent::CommandOutput {
                tab,
                line,
                output: text,
            });
        }
        Req::AskAi { question, context } => {
            let prompt = format!(
                "You are helping a user read the current page in a terminal browser.\nPage context:\n{context}\n\nQuestion: {question}\nAnswer briefly in the user language."
            );
            let answer = match runtime.block_on(cb_core::ai::complete(ctx, "ui", &prompt, 600)) {
                Ok(done) => done.text,
                Err(err) => format!("{}: {}", err.body.code, err.body.message),
            };
            let _ = tx.send(UiEvent::AiAnswer { question, answer });
        }
        Req::Assist { url } => {
            let hint = format!("human assist (T11): cb login {url}");
            let _ = tx.send(UiEvent::Status(hint));
        }
    }
}

fn search_doc(query: &str, results: &[cb_core::SearchResult]) -> RenderDoc {
    let mut blocks = vec![Block::Heading {
        level: 1,
        spans: vec![Span::normal(format!("搜索 / search: {query}"))],
    }];
    let mut links = Vec::new();
    if results.is_empty() {
        blocks.push(Block::Notice {
            level: NoticeLevel::Info,
            title: String::new(),
            spans: vec![Span::normal("没有结果 / no results".to_string())],
        });
    }
    for (idx, result) in results.iter().enumerate() {
        let id = idx as u32;
        links.push(LinkRef {
            id,
            url: result.url.clone(),
            text: result.title.clone(),
        });
        blocks.push(Block::ListItem {
            depth: 0,
            marker: format!("{}. ", idx + 1),
            spans: vec![
                Span {
                    text: format!("[{}] ", result.source),
                    style: cb_extract::SpanStyle::Code,
                    link: None,
                },
                Span {
                    text: result.title.clone(),
                    style: cb_extract::SpanStyle::Bold,
                    link: Some(id),
                },
            ],
        });
        if !result.snippet.is_empty() {
            blocks.push(Block::Paragraph {
                spans: vec![Span {
                    text: result.snippet.clone(),
                    style: cb_extract::SpanStyle::Normal,
                    link: None,
                }],
            });
        }
    }
    RenderDoc {
        title: format!("search: {query}"),
        blocks,
        links,
    }
}
fn load_session(ctx: &Ctx) -> Option<Vec<crate::app::TabSnapshot>> {
    let rows = ctx.store.load_tabs().ok()?;
    if rows.is_empty() {
        return None;
    }
    let mut snapshots = Vec::new();
    for row in rows {
        let nav: Vec<String> =
            serde_json::from_str(&row.nav_json).unwrap_or_else(|_| vec![row.url.clone()]);
        let nav_cursor = row
            .nav_json
            .is_empty()
            .then_some(0usize)
            .unwrap_or_else(|| nav.len().saturating_sub(1));
        snapshots.push(crate::app::TabSnapshot {
            slot: row.slot.max(0) as usize,
            url: row.url,
            title: row.title,
            scroll: row.scroll.max(0) as u16,
            nav,
            nav_cursor,
        });
    }
    Some(snapshots)
}

fn save_session(ctx: &Ctx, app: &crate::App) {
    let rows: Vec<cb_store::TabRow> = app
        .snapshot_tabs()
        .into_iter()
        .map(|snap| cb_store::TabRow {
            slot: snap.slot as i64,
            url: snap.url,
            title: snap.title,
            scroll: snap.scroll as i64,
            nav_json: serde_json::to_string(&snap.nav).unwrap_or_else(|_| "[]".to_string()),
        })
        .collect();
    let _ = ctx.store.save_tabs(&rows);
}
