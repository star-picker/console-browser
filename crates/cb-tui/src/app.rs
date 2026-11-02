//! App：标签页、导航栈、加载代数、浮层与动作执行。
//!
//! 这里不做 IO：动作需要网络/文件时返回 Effect，由驱动层（run 循环）执行。

use crate::action::Action;
use crate::editbuffer::EditBuffer;
use crate::mode::{InputKind, Mode, OverlayKind};
use cb_extract::{notice_doc, NoticeLevel, RenderDoc};
use cb_render::Layout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    Idle,
    Loading,
    Ready,
    Failed { code: String, message: String },
    NeedsHuman { kind: String, hints: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct PageView {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub status: LoadStatus,
    pub cached: bool,
    pub tokens: usize,
    pub raw: String,
    pub markdown: String,
    pub doc: Option<RenderDoc>,
    pub layout: Layout,
}

impl PageView {
    pub fn ready(
        url: impl Into<String>,
        final_url: impl Into<String>,
        title: impl Into<String>,
        doc: RenderDoc,
        width: u16,
        cached: bool,
        tokens: usize,
        raw: impl Into<String>,
        markdown: impl Into<String>,
    ) -> PageView {
        let title = title.into();
        let layout = Layout::new(&doc, width);
        PageView {
            url: url.into(),
            final_url: final_url.into(),
            title,
            status: LoadStatus::Ready,
            cached,
            tokens,
            raw: raw.into(),
            markdown: markdown.into(),
            doc: Some(doc),
            layout,
        }
    }

    fn synthetic(
        width: u16,
        url: &str,
        status: LoadStatus,
        level: NoticeLevel,
        title: &str,
        message: &str,
    ) -> PageView {
        let doc = notice_doc(title, level, message);
        let layout = Layout::new(&doc, width);
        PageView {
            url: url.to_string(),
            final_url: url.to_string(),
            title: title.to_string(),
            status,
            cached: false,
            tokens: 0,
            raw: String::new(),
            markdown: String::new(),
            doc: Some(doc),
            layout,
        }
    }

    pub fn failed(url: &str, width: u16, code: &str, message: &str) -> PageView {
        let status = LoadStatus::Failed {
            code: code.to_string(),
            message: message.to_string(),
        };
        PageView::synthetic(
            width,
            url,
            status,
            NoticeLevel::Error,
            &format!("错误 / Error: {code}"),
            message,
        )
    }

    pub fn needs_human(url: &str, width: u16, kind: &str, hints: &[String]) -> PageView {
        let body = hints.join("\n");
        let status = LoadStatus::NeedsHuman {
            kind: kind.to_string(),
            hints: hints.to_vec(),
        };
        PageView::synthetic(
            width,
            url,
            status,
            NoticeLevel::Warn,
            "需要你 / Needs you",
            &body,
        )
    }

    pub fn relayout(&mut self, width: u16) {
        if let Some(doc) = &self.doc {
            self.layout = Layout::new(doc, width);
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub scroll: u16,
    pub focus: Option<u32>,
}

impl HistoryEntry {
    pub fn new(url: impl Into<String>) -> HistoryEntry {
        HistoryEntry {
            url: url.into(),
            title: String::new(),
            scroll: 0,
            focus: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FindState {
    pub buf: EditBuffer,
    pub hits: Vec<usize>,
    pub current: usize,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub entries: Vec<HistoryEntry>,
    pub cursor: usize,
    pub page: Option<PageView>,
    pub scroll: u16,
    pub focus: Option<u32>,
    pub find: Option<FindState>,
    pub gen: u64,
}

impl Tab {
    pub fn new() -> Tab {
        Tab {
            entries: vec![HistoryEntry::new("")],
            cursor: 0,
            page: None,
            scroll: 0,
            focus: None,
            find: None,
            gen: 0,
        }
    }

    pub fn url(&self) -> &str {
        self.entries
            .get(self.cursor)
            .map(|e| e.url.as_str())
            .unwrap_or("")
    }

    pub fn title(&self) -> String {
        self.page
            .as_ref()
            .map(|p| {
                if p.title.is_empty() {
                    p.url.clone()
                } else {
                    p.title.clone()
                }
            })
            .or_else(|| {
                self.entries
                    .get(self.cursor)
                    .map(|e| e.title.clone())
                    .filter(|t| !t.is_empty())
            })
            .unwrap_or_else(|| "新标签页 / New tab".to_string())
    }

    pub fn total_lines(&self) -> usize {
        self.page
            .as_ref()
            .map(|p| p.layout.total_lines())
            .unwrap_or(0)
    }

    pub fn max_scroll(&self) -> u16 {
        self.total_lines().saturating_sub(1) as u16
    }
}

#[derive(Debug, Clone)]
pub struct OverlayState {
    pub kind: OverlayKind,
    pub items: Vec<OverlayItem>,
    pub cursor: usize,
    pub filter: EditBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayItem {
    pub label: String,
    pub detail: String,
    /// 选中后要去的地方（URL 或内部动作标识）。
    pub target: String,
}

impl OverlayState {
    pub fn new(kind: OverlayKind) -> OverlayState {
        OverlayState {
            kind,
            items: Vec::new(),
            cursor: 0,
            filter: EditBuffer::new(),
        }
    }

    pub fn visible(&self) -> Vec<&OverlayItem> {
        let needle = self.filter.text().to_lowercase();
        if needle.is_empty() {
            return self.items.iter().collect();
        }
        self.items
            .iter()
            .filter(|i| {
                i.label.to_lowercase().contains(&needle)
                    || i.detail.to_lowercase().contains(&needle)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadPage {
        tab: usize,
        url: String,
        gen: u64,
    },
    RunSearch {
        tab: usize,
        engine: String,
        query: String,
        gen: u64,
    },
    ExportMarkdown {
        url: String,
    },
    ExportHtml {
        url: String,
    },
    OpenAssist {
        url: String,
    },
    RunCommand {
        line: String,
    },
    CopyUrl {
        url: String,
    },
    SaveCookies {
        raw: String,
    },
}

pub struct App {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub mode: Mode,
    pub omnibox: EditBuffer,
    pub overlay: Option<OverlayState>,
    pub width: u16,
    pub height: u16,
    pub status: String,
    pub should_quit: bool,
    pub raw_view: bool,
    pub markdown_view: bool,
    pending_gen: u64,
}

impl Default for App {
    fn default() -> Self {
        App::new(120, 30)
    }
}

impl App {
    pub fn new(width: u16, height: u16) -> App {
        let mut omnibox = EditBuffer::new();
        omnibox.set_text("");
        App {
            tabs: vec![Tab::new()],
            active: 0,
            mode: Mode::Input(InputKind::Omnibox),
            omnibox,
            overlay: None,
            width,
            height,
            status: String::new(),
            should_quit: false,
            raw_view: false,
            markdown_view: false,
            pending_gen: 0,
        }
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active.min(self.tabs.len() - 1)]
    }

    pub fn tab_mut(&mut self) -> &mut Tab {
        let idx = self.active.min(self.tabs.len() - 1);
        &mut self.tabs[idx]
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width.max(8);
        self.height = height.max(4);
        for tab in &mut self.tabs {
            if let Some(page) = &mut tab.page {
                page.relayout(self.width);
            }
        }
    }

    /// 提交地址栏/查找输入。
    pub fn submit_input(&mut self) -> Option<Effect> {
        match self.mode.input_kind()? {
            InputKind::Omnibox => {
                let input = self.omnibox.text();
                let input = input.trim().to_string();
                if input.is_empty() {
                    return None;
                }
                self.omnibox.push_history(input.clone());
                match crate::route::classify(&input) {
                    crate::route::Target::Url(url) => Some(self.navigate(&url)),
                    crate::route::Target::Search { engine, query } => {
                        Some(self.run_search(&engine, &query))
                    }
                    crate::route::Target::Command(line) => {
                        self.mode = Mode::Browse;
                        self.status = format!("command: {line}");
                        Some(Effect::RunCommand { line })
                    }
                }
            }
            InputKind::Find => {
                self.mode = Mode::Browse;
                let needle = self.omnibox.text();
                self.start_find(&needle);
                None
            }
            InputKind::Command => {
                let line = self.omnibox.text();
                self.mode = Mode::Browse;
                if line.trim().is_empty() {
                    None
                } else {
                    Some(Effect::RunCommand { line })
                }
            }
            InputKind::PasteCookies => {
                let raw = self.omnibox.text();
                self.mode = Mode::Browse;
                if raw.trim().is_empty() {
                    None
                } else {
                    Some(Effect::SaveCookies { raw })
                }
            }
        }
    }

    pub fn navigate(&mut self, url: &str) -> Effect {
        let width = self.width;
        let tab = self.tab_mut();
        tab.gen += 1;
        let gen = tab.gen;
        // 当前条目之后的前进历史被裁掉（浏览器语义）
        let entry = HistoryEntry::new(url);
        tab.entries.truncate(tab.cursor + 1);
        tab.entries.push(entry);
        tab.cursor = tab.entries.len() - 1;
        tab.page = Some(PageView::synthetic(
            width,
            url,
            LoadStatus::Loading,
            NoticeLevel::Info,
            "loading…",
            "",
        ));
        tab.scroll = 0;
        tab.focus = None;
        self.mode = Mode::Browse;
        self.omnibox.set_text(url);
        Effect::LoadPage {
            tab: self.active,
            url: url.to_string(),
            gen,
        }
    }

    pub fn run_search(&mut self, engine: &str, query: &str) -> Effect {
        let effect = self.navigate(&format!("search://{engine}?{query}"));
        match effect {
            Effect::LoadPage { tab, gen, .. } => Effect::RunSearch {
                tab,
                engine: engine.to_string(),
                query: query.to_string(),
                gen,
            },
            other => other,
        }
    }

    pub fn on_loaded(&mut self, tab_idx: usize, gen: u64, mut view: PageView) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        if tab.gen != gen {
            return; // 过期结果：用户已经取消或又导航了
        }
        view.relayout(self.width);
        if view.title.is_empty() {
            view.title = view.final_url.clone();
        }
        let title = view.title.clone();
        tab.page = Some(view);
        tab.scroll = 0;
        tab.focus = None;
        if let Some(entry) = tab.entries.get_mut(tab.cursor) {
            entry.title = title;
        }
        self.mode = Mode::Browse;
    }

    pub fn on_failed(&mut self, tab_idx: usize, gen: u64, code: &str, message: &str) {
        let width = self.width;
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        if tab.gen != gen {
            return;
        }
        let url = tab.url().to_string();
        tab.page = Some(PageView::failed(&url, width, code, message));
        tab.scroll = 0;
        self.mode = Mode::Browse;
    }

    pub fn on_needs_human(
        &mut self,
        tab_idx: usize,
        gen: u64,
        url: &str,
        kind: &str,
        hints: &[String],
    ) {
        let width = self.width;
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        if tab.gen != gen {
            return;
        }
        tab.page = Some(PageView::needs_human(url, width, kind, hints));
        tab.scroll = 0;
        self.mode = Mode::Browse;
    }

    pub fn back(&mut self) -> Option<Effect> {
        let (url, gen) = {
            let tab = self.tab_mut();
            if tab.cursor == 0 {
                return None;
            }
            if let Some(page) = &tab.page {
                if let Some(entry) = tab.entries.get_mut(tab.cursor) {
                    entry.scroll = tab.scroll;
                    entry.focus = tab.focus;
                    if entry.title.is_empty() {
                        entry.title = page.title.clone();
                    }
                }
            }
            tab.cursor -= 1;
            tab.gen += 1;
            (tab.entries[tab.cursor].url.clone(), tab.gen)
        };
        if url.is_empty() {
            return None;
        }
        let width = self.width;
        let tab = self.tab_mut();
        tab.page = Some(PageView::synthetic(
            width,
            &url,
            LoadStatus::Loading,
            NoticeLevel::Info,
            "loading…",
            "",
        ));
        Some(Effect::LoadPage {
            tab: self.active,
            url,
            gen,
        })
    }

    pub fn forward(&mut self) -> Option<Effect> {
        let (url, gen) = {
            let tab = self.tab_mut();
            if tab.cursor + 1 >= tab.entries.len() {
                return None;
            }
            tab.cursor += 1;
            tab.gen += 1;
            (tab.entries[tab.cursor].url.clone(), tab.gen)
        };
        if url.is_empty() {
            return None;
        }
        Some(Effect::LoadPage {
            tab: self.active,
            url,
            gen,
        })
    }

    pub fn reload(&mut self) -> Option<Effect> {
        let url = {
            let tab = self.tab_mut();
            tab.gen += 1;
            tab.url().to_string()
        };
        if url.is_empty() {
            return None;
        }
        let gen = self.tab().gen;
        Some(Effect::LoadPage {
            tab: self.active,
            url,
            gen,
        })
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let visible = (self.height as u16).saturating_sub(3);
        let tab = self.tab_mut();
        let max = tab.max_scroll().saturating_sub(visible);
        let next = (tab.scroll as i32 + delta).clamp(0, max as i32);
        tab.scroll = next as u16;
    }

    pub fn page_down(&mut self) {
        let step = (self.height as i32 - 3).max(1);
        self.scroll_by(step);
    }

    pub fn page_up(&mut self) {
        let step = (self.height as i32 - 3).max(1);
        self.scroll_by(-step);
    }

    pub fn scroll_top(&mut self) {
        self.tab_mut().scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        let visible = (self.height as u16).saturating_sub(3);
        let tab = self.tab_mut();
        let max = tab.max_scroll().saturating_sub(visible);
        tab.scroll = max;
    }

    pub fn link_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .tab()
            .page
            .as_ref()
            .map(|p| p.layout.link_lines.keys().copied().collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    pub fn focus_next_link(&mut self) {
        let ids = self.link_ids();
        if ids.is_empty() {
            return;
        }
        let tab = self.tab_mut();
        let next_idx = match tab
            .focus
            .and_then(|cur| ids.iter().position(|id| *id == cur))
        {
            Some(pos) => (pos + 1) % ids.len(),
            None => 0,
        };
        tab.focus = Some(ids[next_idx]);
        self.scroll_to_link(ids[next_idx]);
    }

    pub fn focus_prev_link(&mut self) {
        let ids = self.link_ids();
        if ids.is_empty() {
            return;
        }
        let tab = self.tab_mut();
        let next_idx = match tab
            .focus
            .and_then(|cur| ids.iter().position(|id| *id == cur))
        {
            Some(0) => ids.len() - 1,
            Some(pos) => pos - 1,
            None => ids.len() - 1,
        };
        tab.focus = Some(ids[next_idx]);
        self.scroll_to_link(ids[next_idx]);
    }

    fn scroll_to_link(&mut self, id: u32) {
        let line = self
            .tab()
            .page
            .as_ref()
            .and_then(|p| p.layout.link_lines.get(&id).copied());
        if let Some(line) = line {
            let visible = (self.height as usize).saturating_sub(3).max(1);
            let scroll = self.tab().scroll as usize;
            if line < scroll || line >= scroll + visible {
                self.tab_mut().scroll = line.saturating_sub(visible / 2) as u16;
            }
        }
    }

    pub fn focused_url(&self) -> Option<String> {
        let id = self.tab().focus?;
        let page = self.tab().page.as_ref()?;
        page.doc.as_ref()?.link(id).map(|l| l.url.clone())
    }

    pub fn open_focused_link(&mut self, new_tab: bool) -> Option<Effect> {
        let url = self.focused_url()?;
        if new_tab {
            self.new_tab();
        }
        Some(self.navigate(&url))
    }

    pub fn start_find(&mut self, needle: &str) {
        let hits = self
            .tab()
            .page
            .as_ref()
            .map(|p| p.layout.find(needle))
            .unwrap_or_default();
        let tab = self.tab_mut();
        tab.find = Some(FindState {
            buf: EditBuffer::new(),
            hits,
            current: 0,
        });
    }

    pub fn find_next(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return false;
        };
        let Some(find) = &mut tab.find else {
            return false;
        };
        if find.hits.is_empty() {
            return false;
        }
        find.current = (find.current + 1) % find.hits.len();
        let line = find.hits[find.current];
        let visible = (self.height as usize).saturating_sub(3).max(1);
        if line < tab.scroll as usize || line >= tab.scroll as usize + visible {
            tab.scroll = line.saturating_sub(visible / 2) as u16;
        }
        true
    }

    pub fn find_prev(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return false;
        };
        let Some(find) = &mut tab.find else {
            return false;
        };
        if find.hits.is_empty() {
            return false;
        }
        find.current = if find.current == 0 {
            find.hits.len() - 1
        } else {
            find.current - 1
        };
        let line = find.hits[find.current];
        let visible = (self.height as usize).saturating_sub(3).max(1);
        if line < tab.scroll as usize || line >= tab.scroll as usize + visible {
            tab.scroll = line.saturating_sub(visible / 2) as u16;
        }
        true
    }

    pub fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active = self.tabs.len() - 1;
        self.mode = Mode::Input(InputKind::Omnibox);
        self.omnibox.clear();
    }

    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            self.tabs[0] = Tab::new();
            self.active = 0;
            self.mode = Mode::Input(InputKind::Omnibox);
            self.omnibox.clear();
            return;
        }
        self.tabs.remove(self.active);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = (self.active + 1) % self.tabs.len();
        self.sync_omnibox_from_tab();
    }

    pub fn select_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
            self.sync_omnibox_from_tab();
        }
    }

    fn sync_omnibox_from_tab(&mut self) {
        let url = self.tab().url().to_string();
        self.omnibox.set_text(url);
    }

    pub fn open_overlay(&mut self, kind: OverlayKind) {
        self.overlay = Some(OverlayState::new(kind));
        self.mode = Mode::Overlay(kind);
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
        self.mode = Mode::Browse;
    }

    pub fn overlay_down(&mut self) {
        if let Some(overlay) = &mut self.overlay {
            let len = overlay.visible().len();
            if len > 0 {
                overlay.cursor = (overlay.cursor + 1) % len;
            }
        }
    }

    pub fn overlay_up(&mut self) {
        if let Some(overlay) = &mut self.overlay {
            let len = overlay.visible().len();
            if len > 0 {
                overlay.cursor = if overlay.cursor == 0 {
                    len - 1
                } else {
                    overlay.cursor - 1
                };
            }
        }
    }

    /// 选中浮层条目：返回要打开的 URL（如果有）。
    pub fn overlay_select(&mut self) -> Option<String> {
        let overlay = self.overlay.take()?;
        let target = overlay
            .visible()
            .get(overlay.cursor)
            .map(|i| i.target.clone());
        self.mode = Mode::Browse;
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(80, 24)
    }

    fn loaded(app: &mut App, url: &str) {
        let gen = app.tab().gen;
        let doc = notice_doc("ok", NoticeLevel::Info, "hello");
        let mut view = PageView::ready(url, url, url, doc, app.width, false, 1, "", "");
        view.relayout(app.width);
        app.on_loaded(0, gen, view);
    }

    #[test]
    fn navigate_pushes_history_and_back_returns() {
        let mut app = app();
        app.navigate("https://a.example/1");
        loaded(&mut app, "https://a.example/1");
        app.navigate("https://a.example/2");
        loaded(&mut app, "https://a.example/2");
        assert_eq!(app.tab().url(), "https://a.example/2");
        let effect = app.back().expect("back effect");
        assert!(matches!(effect, Effect::LoadPage { .. }));
        assert_eq!(app.tab().url(), "https://a.example/1");
        assert!(app.forward().is_some());
        assert_eq!(app.tab().url(), "https://a.example/2");
        assert!(app.forward().is_none(), "no forward history left");
    }

    #[test]
    fn stale_load_results_are_ignored() {
        let mut app = app();
        app.navigate("https://a.example/1");
        let old_gen = app.tab().gen;
        app.navigate("https://a.example/2");
        let doc = notice_doc("stale", NoticeLevel::Info, "old");
        let view = PageView::ready(
            "https://a.example/1",
            "https://a.example/1",
            "old",
            doc,
            app.width,
            false,
            1,
            "",
            "",
        );
        app.on_loaded(0, old_gen, view);
        assert_eq!(
            app.tab().url(),
            "https://a.example/2",
            "stale load must not win"
        );
    }

    #[test]
    fn closing_last_tab_leaves_a_fresh_tab() {
        let mut app = app();
        app.navigate("https://a.example/1");
        app.close_tab();
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tab().url(), "");
        assert!(app.tab().page.is_none());
    }

    #[test]
    fn tab_management_switches_and_closes() {
        let mut app = app();
        app.navigate("https://a.example/1");
        app.new_tab();
        assert_eq!(app.tabs.len(), 2);
        app.navigate("https://b.example/2");
        app.next_tab();
        assert_eq!(app.active, 0);
        app.select_tab(1);
        assert_eq!(app.active, 1);
        app.close_tab();
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn link_focus_cycles_through_links() {
        let mut app = app();
        let link = |id: u32| cb_extract::Span {
            text: format!("l{id}"),
            style: cb_extract::SpanStyle::Normal,
            link: Some(id),
        };
        let doc = RenderDoc {
            title: "t".to_string(),
            blocks: vec![
                cb_extract::Block::Paragraph {
                    spans: vec![link(0)],
                },
                cb_extract::Block::Paragraph {
                    spans: vec![link(1)],
                },
            ],
            links: vec![
                cb_extract::LinkRef {
                    id: 0,
                    url: "https://a.example/0".to_string(),
                    text: "l0".to_string(),
                },
                cb_extract::LinkRef {
                    id: 1,
                    url: "https://a.example/1".to_string(),
                    text: "l1".to_string(),
                },
            ],
        };
        let gen = app.tab().gen;
        let view = PageView::ready(
            "https://a.example/",
            "https://a.example/",
            "t",
            doc,
            app.width,
            false,
            1,
            "",
            "",
        );
        app.on_loaded(0, gen, view);
        assert_eq!(app.link_ids(), vec![0, 1]);
        app.focus_next_link();
        assert_eq!(app.tab().focus, Some(0));
        assert_eq!(app.focused_url().as_deref(), Some("https://a.example/0"));
        app.focus_next_link();
        assert_eq!(app.tab().focus, Some(1));
        app.focus_next_link();
        assert_eq!(app.tab().focus, Some(0), "wraps");
        app.focus_prev_link();
        assert_eq!(app.tab().focus, Some(1), "wraps backwards");
    }

    #[test]
    fn scroll_is_clamped_to_page_bounds() {
        let mut app = app();
        let doc = RenderDoc {
            title: "t".to_string(),
            blocks: (0..200)
                .map(|i| cb_extract::Block::Paragraph {
                    spans: vec![cb_extract::Span::normal(format!("line {i}"))],
                })
                .collect(),
            links: Vec::new(),
        };
        let gen = app.tab().gen;
        let view = PageView::ready(
            "https://a.example/",
            "https://a.example/",
            "t",
            doc,
            app.width,
            false,
            1,
            "",
            "",
        );
        app.on_loaded(0, gen, view);
        app.scroll_by(-5);
        assert_eq!(app.tab().scroll, 0);
        app.scroll_bottom();
        let bottom = app.tab().scroll;
        assert!(bottom > 0);
        app.scroll_by(1000);
        assert_eq!(app.tab().scroll, bottom);
        app.scroll_top();
        assert_eq!(app.tab().scroll, 0);
    }

    #[test]
    fn failed_page_renders_notice_and_keeps_ui_usable() {
        let mut app = app();
        app.navigate("https://a.example/first");
        loaded(&mut app, "https://a.example/first");
        app.navigate("https://a.example/404");
        let gen = app.tab().gen;
        app.on_failed(0, gen, "not_found", "http 404");
        let page = app.tab().page.as_ref().expect("page");
        assert!(matches!(page.status, LoadStatus::Failed { .. }));
        assert!(page.layout.total_lines() > 0);
        assert!(page
            .layout
            .lines
            .iter()
            .any(|l| l.text().contains("not_found")));
        assert!(page.layout.lines.iter().any(|l| l.text().contains("404")));
        assert!(app.back().is_some());
    }

    #[test]
    fn needs_human_page_shows_hints() {
        let mut app = app();
        app.navigate("https://a.example/login");
        let gen = app.tab().gen;
        app.on_needs_human(
            0,
            gen,
            "https://a.example/login",
            "captcha",
            &["press e".to_string()],
        );
        let page = app.tab().page.as_ref().expect("page");
        assert!(matches!(page.status, LoadStatus::NeedsHuman { .. }));
        let text: String = page.layout.lines.iter().map(|l| l.text()).collect();
        assert!(text.contains("press e"), "hints rendered: {text}");
    }

    #[test]
    fn overlays_open_close_and_select() {
        let mut app = app();
        app.open_overlay(OverlayKind::Links);
        assert!(matches!(app.mode, Mode::Overlay(OverlayKind::Links)));
        if let Some(overlay) = &mut app.overlay {
            overlay.items.push(OverlayItem {
                label: "a".to_string(),
                detail: "d".to_string(),
                target: "https://x/".to_string(),
            });
            overlay.items.push(OverlayItem {
                label: "b".to_string(),
                detail: "d".to_string(),
                target: "https://y/".to_string(),
            });
        }
        app.overlay_down();
        let target = app.overlay_select().expect("target");
        assert_eq!(target, "https://y/");
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn find_cycles_hits() {
        let mut app = app();
        let doc = RenderDoc {
            title: "t".to_string(),
            blocks: vec![
                cb_extract::Block::Paragraph {
                    spans: vec![cb_extract::Span::normal("needle one")],
                },
                cb_extract::Block::Paragraph {
                    spans: vec![cb_extract::Span::normal("needle two")],
                },
            ],
            links: Vec::new(),
        };
        let gen = app.tab().gen;
        let view = PageView::ready(
            "https://a.example/",
            "https://a.example/",
            "t",
            doc,
            app.width,
            false,
            1,
            "",
            "",
        );
        app.on_loaded(0, gen, view);
        app.start_find("needle");
        assert_eq!(app.tab().find.as_ref().expect("find").hits.len(), 2);
        assert!(app.find_next());
        assert!(app.find_next());
        assert!(app.find_prev());
    }
}
impl App {
    /// 执行一个动作；需要 IO 时返回 Effect。
    pub fn apply(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::FocusOmnibox => {
                self.mode = Mode::Input(InputKind::Omnibox);
                let url = self.tab().url().to_string();
                self.omnibox.set_text(url);
                None
            }
            Action::FocusFind => {
                self.mode = Mode::Input(InputKind::Find);
                self.omnibox.clear();
                None
            }
            Action::FocusCommand => {
                self.mode = Mode::Input(InputKind::Command);
                self.omnibox.clear();
                None
            }
            Action::PasteCookies => {
                self.mode = Mode::Input(InputKind::PasteCookies);
                self.omnibox.clear();
                None
            }
            Action::Submit => self.submit_input(),
            Action::CancelInput => {
                self.mode = Mode::Browse;
                let url = self.tab().url().to_string();
                self.omnibox.set_text(url);
                None
            }
            Action::ClearInput => {
                self.omnibox.clear();
                None
            }
            Action::EditBackspace => {
                self.omnibox.backspace();
                None
            }
            Action::EditWordBackspace => {
                self.omnibox.delete_word();
                None
            }
            Action::EditLeft => {
                self.omnibox.left();
                None
            }
            Action::EditRight => {
                self.omnibox.right();
                None
            }
            Action::EditHome => {
                self.omnibox.home();
                None
            }
            Action::EditEnd => {
                self.omnibox.end();
                None
            }
            Action::HistoryPrev => {
                self.omnibox.history_prev();
                None
            }
            Action::HistoryNext => {
                self.omnibox.history_next();
                None
            }
            Action::Open(url) => Some(self.navigate(&url)),
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Reload => {
                self.status = "reload".to_string();
                self.reload()
            }
            Action::Home => {
                self.mode = Mode::Input(InputKind::Omnibox);
                self.omnibox.clear();
                None
            }
            Action::ScrollDown(n) => {
                self.scroll_by(n.max(1));
                None
            }
            Action::ScrollUp(n) => {
                self.scroll_by(-n.max(1));
                None
            }
            Action::PageDown => {
                self.page_down();
                None
            }
            Action::PageTop => {
                self.scroll_top();
                None
            }
            Action::PageBottom => {
                self.scroll_bottom();
                None
            }
            Action::FocusNextLink => {
                self.focus_next_link();
                None
            }
            Action::FocusPrevLink => {
                self.focus_prev_link();
                None
            }
            Action::OpenFocusedLink { new_tab } => self.open_focused_link(new_tab),
            Action::ToggleRawView => {
                self.raw_view = !self.raw_view;
                self.markdown_view = false;
                None
            }
            Action::ToggleMarkdownView => {
                self.markdown_view = !self.markdown_view;
                self.raw_view = false;
                None
            }
            Action::FindNext => {
                if !self.find_next() {
                    self.status = "no find matches".to_string();
                }
                None
            }
            Action::FindPrev => {
                if !self.find_prev() {
                    self.status = "no find matches".to_string();
                }
                None
            }
            Action::TabNew => {
                self.new_tab();
                None
            }
            Action::TabClose => {
                self.close_tab();
                None
            }
            Action::TabNext => {
                self.next_tab();
                None
            }
            Action::TabPrev => {
                if self.tabs.len() > 1 {
                    let prev = if self.active == 0 {
                        self.tabs.len() - 1
                    } else {
                        self.active - 1
                    };
                    self.select_tab(prev);
                }
                None
            }
            Action::TabSelect(idx) => {
                self.select_tab(idx);
                None
            }
            Action::ToggleHelp => {
                self.open_overlay(OverlayKind::Help);
                None
            }
            Action::ToggleHistory => {
                self.open_overlay(OverlayKind::History);
                None
            }
            Action::ToggleLinks => {
                self.open_overlay(OverlayKind::Links);
                None
            }
            Action::TogglePalette => {
                self.open_overlay(OverlayKind::Palette);
                None
            }
            Action::ToggleAi => {
                self.open_overlay(OverlayKind::Ai);
                None
            }
            Action::ToggleTrace => {
                self.open_overlay(OverlayKind::Trace);
                None
            }
            Action::OverlayUp => {
                self.overlay_up();
                None
            }
            Action::OverlayDown => {
                self.overlay_down();
                None
            }
            Action::OverlaySelect => {
                if let Some(target) = self.overlay_select() {
                    if target.starts_with("https://") || target.starts_with("http://") {
                        return Some(self.navigate(&target));
                    }
                    self.status = format!("selected: {target}");
                }
                None
            }
            Action::OverlayFilter => {
                self.mode = Mode::Input(InputKind::Find);
                self.omnibox.clear();
                None
            }
            Action::CloseOverlay => {
                self.close_overlay();
                None
            }
            Action::OpenHumanAssist => {
                let url = self.tab().url().to_string();
                if url.is_empty() {
                    self.status = "nothing loaded".to_string();
                    None
                } else {
                    Some(Effect::OpenAssist { url })
                }
            }
            Action::ExportMarkdown => {
                let url = self.tab().url().to_string();
                Some(Effect::ExportMarkdown { url })
            }
            Action::ExportHtml => {
                let url = self.tab().url().to_string();
                Some(Effect::ExportHtml { url })
            }
            Action::CopyUrl => {
                let url = self.tab().url().to_string();
                Some(Effect::CopyUrl { url })
            }
            Action::SwitchLang => {
                let next = cb_core::i18n::current().next();
                cb_core::i18n::set(next);
                self.status = format!("lang: {}", next.as_str());
                None
            }
            Action::Quit => {
                self.should_quit = true;
                None
            }
            Action::ForceQuit => {
                self.should_quit = true;
                None
            }
            Action::MouseClick(_, _) => None,
            Action::MouseScroll(up) => {
                if up {
                    self.scroll_by(-3);
                } else {
                    self.scroll_by(3);
                }
                None
            }
        }
    }
}
