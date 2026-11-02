//! 视图：只负责画，不改状态。

use crate::app::{App, LoadStatus, OverlayState};
use crate::mode::{InputKind, Mode, OverlayKind};
use cb_render::Style as RdStyle;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TLine, Span as TSpan};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const ACCENT2: Color = Color::Magenta;
const DIM: Color = Color::DarkGray;
const WARN: Color = Color::Yellow;
const ERR: Color = Color::Red;

fn map_style(style: RdStyle, focused: bool, found: bool) -> Style {
    let mut out = match style {
        RdStyle::Normal => Style::default(),
        RdStyle::Bold => Style::default().add_modifier(Modifier::BOLD),
        RdStyle::Italic => Style::default().add_modifier(Modifier::ITALIC),
        RdStyle::Code => Style::default().fg(WARN),
        RdStyle::Heading => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        RdStyle::Dim => Style::default().fg(DIM),
        RdStyle::Rule => Style::default().fg(DIM),
        RdStyle::NoticeInfo => Style::default().fg(ACCENT),
        RdStyle::NoticeWarn => Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        RdStyle::NoticeError => Style::default().fg(ERR).add_modifier(Modifier::BOLD),
    };
    if focused {
        out = out.add_modifier(Modifier::REVERSED);
    }
    if found {
        out = out.bg(WARN).fg(Color::Black);
    }
    out
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 {
        let msg = Paragraph::new("terminal too small / 终端太小").style(Style::default().fg(ERR));
        frame.render_widget(msg, area);
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    draw_omnibox(frame, app, chunks[0]);
    draw_tabbar(frame, app, chunks[1]);
    draw_body(frame, app, chunks[2]);
    draw_status(frame, app, chunks[3]);

    if let Some(overlay) = &app.overlay {
        draw_overlay(frame, app, overlay, area);
    }
}

fn draw_omnibox(frame: &mut Frame, app: &App, area: Rect) {
    let focused = matches!(app.mode, Mode::Input(InputKind::Omnibox));
    let label = if focused { "▸ " } else { "  " };
    let text = app.omnibox.text();
    let shown = if text.is_empty() {
        "输入网址或搜索… / type a URL or search…".to_string()
    } else {
        text.clone()
    };
    let base = if text.is_empty() {
        Style::default().fg(DIM)
    } else if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    let line = TLine::from(vec![
        TSpan::styled(
            label,
            Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD),
        ),
        TSpan::styled(shown, base),
    ]);
    frame.render_widget(Paragraph::new(line), area);
    if focused {
        let x = area.x + 2 + app.omnibox.cursor() as u16;
        let _ =
            frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), area.y));
    }
}

fn draw_tabbar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<TSpan> = Vec::new();
    for (idx, tab) in app.tabs.iter().enumerate() {
        if idx > 0 {
            spans.push(TSpan::styled(" │ ", Style::default().fg(DIM)));
        }
        let title = tab.title();
        let short: String = if title.chars().count() > 18 {
            title.chars().take(17).collect::<String>() + "…"
        } else {
            title
        };
        let label = format!(" {} {} ", idx + 1, short);
        let style = if idx == app.active {
            Style::default()
                .bg(ACCENT)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        spans.push(TSpan::styled(label, style));
    }
    frame.render_widget(Paragraph::new(TLine::from(spans)), area);
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let tab = app.tab();
    let Some(page) = &tab.page else {
        let hint = TLine::from(vec![
            TSpan::styled(
                "按 l 输入地址，或直接输入搜索词。",
                Style::default().fg(DIM),
            ),
            TSpan::raw("  "),
            TSpan::styled(
                "Press l to type, or just type a query.",
                Style::default().fg(DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(hint).wrap(Wrap { trim: true }), area);
        return;
    };

    if app.raw_view && !page.raw.is_empty() {
        let text = page.raw.clone();
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    if app.markdown_view && !page.markdown.is_empty() {
        let text = page.markdown.clone();
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }

    let focus = tab.focus;
    let find_hits: Vec<usize> = tab
        .find
        .as_ref()
        .map(|f| f.hits.clone())
        .unwrap_or_default();
    let find_current = tab
        .find
        .as_ref()
        .and_then(|f| f.hits.get(f.current).copied());

    let visible_height = area.height as usize;
    let scroll = tab.scroll as usize;
    let mut lines: Vec<TLine> = Vec::with_capacity(visible_height);
    for (offset, line) in page
        .layout
        .lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .enumerate()
    {
        let line_no = scroll + offset;
        let found = find_hits.contains(&line_no);
        let is_current_match = find_current == Some(line_no);
        let cells: Vec<TSpan> = line
            .cells
            .iter()
            .map(|cell| {
                let focused = cell.link.is_some() && cell.link == focus;
                let found_here = found && !is_current_match;
                TSpan::styled(
                    cell.text.clone(),
                    map_style(cell.style, focused, found_here),
                )
            })
            .collect();
        if !cells.is_empty() && is_current_match {
            lines.push(TLine::from(
                cells
                    .into_iter()
                    .map(|span| {
                        TSpan::styled(
                            span.content.to_string(),
                            span.style.bg(WARN).fg(Color::Black),
                        )
                    })
                    .collect::<Vec<_>>(),
            ));
        } else if cells.is_empty() {
            lines.push(TLine::from(""));
        } else {
            lines.push(TLine::from(cells));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let tab = app.tab();
    let page = &tab.page;
    let mode_label = match app.mode {
        Mode::Browse => "BROWSE",
        Mode::Input(InputKind::Omnibox) => "URL",
        Mode::Input(InputKind::Find) => "FIND",
        Mode::Input(InputKind::Command) => "CMD",
        Mode::Input(InputKind::PasteCookies) => "COOKIE",
        Mode::Input(InputKind::AiPrompt) => "AI",
        Mode::Overlay(kind) => match kind {
            OverlayKind::Help => "HELP",
            OverlayKind::History => "HISTORY",
            OverlayKind::Links => "LINKS",
            OverlayKind::Palette => "PALETTE",
            OverlayKind::Ai => "AI",
            OverlayKind::Trace => "TRACE",
        },
    };
    let mut left: Vec<TSpan> = vec![TSpan::styled(
        format!(" {mode_label} "),
        Style::default()
            .bg(ACCENT2)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(page) = page {
        let total = page.layout.total_lines().max(1);
        let pct = ((tab.scroll as usize * 100) / total).min(100);
        let state = match &page.status {
            LoadStatus::Idle => "idle".to_string(),
            LoadStatus::Loading => "loading…".to_string(),
            LoadStatus::Ready => {
                if page.cached {
                    "cache".to_string()
                } else {
                    format!("{pct}%")
                }
            }
            LoadStatus::Failed { code, .. } => format!("error:{code}"),
            LoadStatus::NeedsHuman { kind, .. } => format!("needs:{kind}"),
        };
        left.push(TSpan::styled(
            format!(" {state} "),
            Style::default().fg(ACCENT),
        ));
        left.push(TSpan::styled(
            format!("· {}行", page.layout.total_lines()),
            Style::default().fg(DIM),
        ));
        if page.tokens > 0 {
            left.push(TSpan::styled(
                format!(" · {}tok", page.tokens),
                Style::default().fg(DIM),
            ));
        }
    }
    if let Some(url) = app.focused_url() {
        left.push(TSpan::styled("  → ", Style::default().fg(DIM)));
        left.push(TSpan::styled(url, Style::default().fg(ACCENT)));
    }
    if let Some(find) = &tab.find {
        if !find.hits.is_empty() {
            left.push(TSpan::styled(
                format!("  [{}/{}]", find.current + 1, find.hits.len()),
                Style::default().fg(WARN),
            ));
        }
    }
    if !app.status.is_empty() {
        left.push(TSpan::styled(
            format!("  {}", app.status),
            Style::default().fg(DIM),
        ));
    }
    frame.render_widget(Paragraph::new(TLine::from(left)), area);
}

fn draw_overlay(frame: &mut Frame, app: &App, overlay: &OverlayState, area: Rect) {
    let width = (area.width as u32 * 8 / 10).max(40) as u16;
    let height = (area.height as u32 * 7 / 10).max(10) as u16;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };
    let title = match overlay.kind {
        OverlayKind::Help => "帮助 / Help (Esc 关闭)",
        OverlayKind::History => "历史 / History",
        OverlayKind::Links => "链接 / Links",
        OverlayKind::Palette => "命令面板 / Palette",
        OverlayKind::Ai => "AI 面板 / AI",
        OverlayKind::Trace => "日志 / Trace",
    };
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(Style::default().fg(ACCENT));
    frame.render_widget(Clear, rect);
    frame.render_widget(block.clone(), rect);
    let inner = block.inner(rect);
    let lines: Vec<TLine> = match overlay.kind {
        OverlayKind::Help => help_lines(),
        _ => {
            let visible = overlay.visible();
            if visible.is_empty() {
                vec![TLine::from(vec![TSpan::styled(
                    "(空 / empty)",
                    Style::default().fg(DIM),
                )])]
            } else {
                visible
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let selected = idx == overlay.cursor;
                        let style = if selected {
                            Style::default().bg(ACCENT).fg(Color::Black)
                        } else {
                            Style::default()
                        };
                        TLine::from(vec![
                            TSpan::styled(format!("{} ", if selected { "▸" } else { " " }), style),
                            TSpan::styled(item.label.clone(), style.add_modifier(Modifier::BOLD)),
                            TSpan::styled(format!("  {}", item.detail), style.fg(DIM)),
                        ])
                    })
                    .collect()
            }
        }
    };
    frame.render_widget(Paragraph::new(lines).block(Block::default()), inner);
    let _ = app;
}

fn help_lines() -> Vec<TLine<'static>> {
    let rows = [
        ("l / Ctrl-L", "地址栏 / omnibox"),
        ("Enter", "打开聚焦链接 / open focused link"),
        ("Tab / Shift-Tab", "链接焦点 / cycle links"),
        ("b / f / r", "后退 / 前进 / 重载"),
        ("j k Space PgDn PgUp g G", "滚动 / scroll"),
        ("/ n N", "查找 / find next-prev"),
        ("Ctrl-H Ctrl-P Ctrl-A L", "历史 / 面板 / AI / 链接"),
        (":", "命令行 / command line"),
        ("e / c", "人工接力 / 粘贴 cookie"),
        ("Ctrl-T / Ctrl-W / Alt-1..9", "标签 / tabs"),
        ("v / Ctrl-R", "原始文本 / markdown"),
        ("Ctrl-E / Ctrl-S", "导出 md / html"),
        ("y / ? / q", "复制 URL / 帮助 / 退出"),
    ];
    rows.iter()
        .map(|(keys, what)| {
            TLine::from(vec![
                TSpan::styled(format!(" {keys:<26}"), Style::default().fg(ACCENT2)),
                TSpan::styled(*what, Style::default().fg(Color::Gray)),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Effect, PageView};
    use cb_extract::{notice_doc, NoticeLevel, RenderDoc};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_sized(app: &App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| draw(frame, app)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn render(app: &App) -> String {
        render_sized(app, 100, 30)
    }

    #[test]
    fn empty_state_shows_placeholder_and_hints() {
        let app = App::new(100, 30);
        let screen = render(&app);
        assert!(screen.contains("type a URL or search"), "{screen}");
        assert!(
            screen.contains("BROWSE") || screen.contains("URL"),
            "{screen}"
        );
    }

    #[test]
    fn loaded_page_renders_heading_and_status() {
        let mut app = App::new(100, 30);
        let doc = RenderDoc {
            title: "T".to_string(),
            blocks: vec![cb_extract::Block::Heading {
                level: 1,
                spans: vec![cb_extract::Span::normal("Terminal Browser")],
            }],
            links: Vec::new(),
        };
        let gen = app.tab().gen;
        app.on_loaded(
            0,
            gen,
            PageView::ready("https://a/", "https://a/", "T", doc, 100, false, 42, "", ""),
        );
        let screen = render(&app);
        assert!(screen.contains("Terminal Browser"), "{screen}");
        assert!(screen.contains("42tok"), "{screen}");
    }

    #[test]
    fn failed_page_renders_error_notice() {
        let mut app = App::new(100, 30);
        app.navigate("https://a/404");
        let gen = app.tab().gen;
        app.on_failed(0, gen, "not_found", "http 404");
        let screen = render(&app);
        assert!(screen.contains("not_found"), "{screen}");
        assert!(screen.contains("error:"), "{screen}");
    }

    #[test]
    fn needs_human_page_renders_hints_and_activates() {
        let mut app = App::new(100, 30);
        app.navigate("https://a/login");
        let gen = app.tab().gen;
        app.on_needs_human(
            0,
            gen,
            "https://a/login",
            "captcha",
            &["press e to open the human assist window".to_string()],
        );
        let screen = render(&app);
        assert!(screen.contains("Needs you"), "{screen}");
        assert!(screen.contains("press e"), "{screen}");
        assert!(screen.contains("needs:captcha"), "{screen}");
    }

    #[test]
    fn help_overlay_lists_key_bindings() {
        let mut app = App::new(100, 30);
        app.open_overlay(OverlayKind::Help);
        let screen = render(&app);
        assert!(
            screen.contains('┌') || screen.contains('╭'),
            "overlay box missing: {screen}"
        );
        assert!(screen.contains("omnibox"), "{screen}");
        assert!(screen.contains("Ctrl-H"), "{screen}");
    }

    #[test]
    fn tiny_terminal_shows_a_guard_message() {
        let app = App::new(20, 5);
        let screen = render_sized(&app, 20, 5);
        assert!(screen.contains("too small"), "{screen}");
    }

    #[test]
    fn raw_view_toggle_is_ignored_when_page_has_no_raw() {
        let mut app = App::new(100, 30);
        let doc = notice_doc("t", NoticeLevel::Info, "hello");
        let gen = app.tab().gen;
        app.on_loaded(
            0,
            gen,
            PageView::ready("https://a/", "https://a/", "t", doc, 100, false, 0, "", ""),
        );
        app.raw_view = true;
        let screen = render(&app);
        assert!(screen.contains("hello"), "{screen}");
        let _: Option<Effect> = None;
    }
}
