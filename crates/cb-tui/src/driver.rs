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
    Status(String),
}

pub fn run(ctx: Ctx, opts: DriverOpts) -> anyhow::Result<i32> {
    let mut screen = Screen::enter(opts.mouse)?;
    let (width, height) = screen.size();
    let mut app = App::new(width, height);

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
        Effect::OpenAssist { .. } => {
            app.status = "human assist: 见 T11（cb login）".to_string();
        }
        Effect::RunCommand { line } => {
            app.status = format!("command: {line}（T10 接原 CLI）");
        }
        Effect::CopyUrl { url } => {
            app.status = format!("url: {url}");
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
        UiEvent::Status(msg) => app.status = msg,
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
