//! 轻量 i18n：中英两套文案 + 语言探测。
//!
//! 覆盖面：运行时消息与交互式外壳。CLI 的 flag 帮助文案采用「中英同行」写法，
//! 一眼看两种语言，不需要运行时切换。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn parse(raw: &str) -> Option<Lang> {
        let v = raw.trim().to_ascii_lowercase();
        match v.as_str() {
            "zh" | "zh-cn" | "zh-hans" | "cn" | "chinese" | "中文" => Some(Lang::Zh),
            "en" | "en-us" | "en-gb" | "english" => Some(Lang::En),
            _ => {
                if v.starts_with("zh") {
                    Some(Lang::Zh)
                } else if v.starts_with("en") {
                    Some(Lang::En)
                } else {
                    None
                }
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    pub fn next(self) -> Lang {
        match self {
            Lang::Zh => Lang::En,
            Lang::En => Lang::Zh,
        }
    }

    fn index(self) -> u8 {
        match self {
            Lang::Zh => 0,
            Lang::En => 1,
        }
    }

    fn from_index(i: u8) -> Lang {
        if i == 1 {
            Lang::En
        } else {
            Lang::Zh
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

pub fn set(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

/// 探测顺序：显式参数 > CB_LANG 环境变量 > 系统区域 > 中文默认。
pub fn detect(explicit: Option<&str>) -> Lang {
    if let Some(raw) = explicit {
        if let Some(lang) = Lang::parse(raw) {
            return lang;
        }
    }
    if let Ok(raw) = std::env::var("CB_LANG") {
        if let Some(lang) = Lang::parse(&raw) {
            return lang;
        }
    }
    if let Some(raw) = sys_locale::get_locale() {
        if let Some(lang) = Lang::parse(&raw) {
            return lang;
        }
    }
    Lang::Zh
}

/// (key, 中文, English)
pub const CATALOG: &[(&str, &str, &str)] = &[
    ("shell.banner", "交互模式。输入 help 看命令，exit 退出。", "Interactive mode. Type help for commands, exit to quit."),
    ("shell.prompt", "cb> ", "cb> "),
    ("shell.goodbye", "再见。", "Bye."),
    ("shell.help", "命令：fetch / export / history / tools / selftest / db / mcp。可用 lang zh|en 切换语言，exit 退出。也可以直接敲 cb <命令> --help。", "Commands: fetch / export / history / tools / selftest / db / mcp. Use lang zh|en to switch language, exit to quit. Each command answers --help."),
    ("shell.unknown", "看不懂这条命令。输入 help 看看能做什么。", "Unknown command. Type help to see what is available."),
    ("shell.lang", "语言已切换 / language switched: ", "language switched: "),
    ("shell.parse_error", "参数有问题 / bad arguments:", "bad arguments:"),
    ("common.wrote", "已写入", "wrote"),
    ("common.bytes", "字节", "bytes"),
    ("db.pruned", "已清理", "pruned"),
    ("db.stats", "行数统计", "row counts"),
    ("selftest.all_ok", "自检全部通过", "selftest passed"),
    ("selftest.failed", "自检有失败项", "selftest has failures"),
    ("export.no_content", "没有内容可导出", "nothing to export"),

    // ---- TUI 文案（单语言切换：选中语言后只显示这一种）----
    ("ui.placeholder", "输入网址或搜索…", "Type a URL or search…"),
    ("ui.empty.line1", "直接输入网址或搜索词，回车加载。", "Type a URL or a query, then press Enter."),
    ("ui.empty.line2", "地址栏输入 ? 或 cb:help 看全部键位；cb:history 看历史。", "Type ? or cb:help in the URL bar to see all keys; cb:history opens history."),
    ("ui.empty.keys", "导航 l Enter Tab b f r Esc · 标签 Ctrl-T Ctrl-W Alt-1..9 · 工具 Ctrl-H Ctrl-P Ctrl-A : ?", "navigate l Enter Tab b f r Esc · tabs Ctrl-T Ctrl-W Alt-1..9 · tools Ctrl-H Ctrl-P Ctrl-A : ?"),
    ("ui.mode.browse", "浏览", "BROWSE"),
    ("ui.mode.url", "地址", "URL"),
    ("ui.mode.find", "查找", "FIND"),
    ("ui.mode.command", "命令", "CMD"),
    ("ui.mode.cookie", "COOKIE", "COOKIE"),
    ("ui.mode.ai", "AI", "AI"),
    ("ui.overlay.help", "帮助", "Help"),
    ("ui.overlay.history", "历史", "History"),
    ("ui.overlay.links", "链接", "Links"),
    ("ui.overlay.palette", "命令面板", "Palette"),
    ("ui.overlay.ai", "AI 面板", "AI"),
    ("ui.overlay.trace", "日志", "Trace"),
    ("ui.overlay.empty", "（空）", "(empty)"),
    ("ui.overlay.close", "Esc 关闭", "Esc closes"),
    ("ui.loading", "加载中…", "loading…"),
    ("ui.cancelled", "已取消", "cancelled"),
    ("ui.newtab", "新标签页", "New tab"),
    ("ui.needs_login", "需要登录", "login required"),
    ("ui.no_results", "没有结果", "no results"),
    ("ui.no_history", "没有历史记录", "no history"),
    ("ui.no_cookies", "还没有保存 cookie", "no cookies stored"),
    ("ui.cookies_cleared", "已清空 cookie", "cleared cookies"),
    ("ui.assist_hint", "按 e 打开人工窗口登录；按 c 粘贴 Cookie", "press e to open the human assist window; press c to paste cookies"),
    ("ui.status.exported", "已导出", "exported"),
    ("ui.status.export_failed", "导出失败", "export failed"),
    ("ui.status.cookies_so_far", "已收集 cookie", "cookies so far"),
    ("ui.status.assist_window", "人工窗口", "assist window"),
    ("ui.status.assist_done", "登录完成，已保存 cookie，正在重载", "login captured, cookies saved, reloading"),
    ("ui.status.assist_none", "人工窗口没拿到 cookie；可改用 c 粘贴", "assist finished without cookies; try c to paste them"),
    ("ui.status.url", "链接", "url"),
    ("ui.help.title", "帮助 / 键位", "Help / keys"),
    ("ui.help.omnibox", "地址栏", "omnibox"),
    ("ui.help.open", "打开聚焦链接", "open focused link"),
    ("ui.help.focus", "链接焦点循环", "cycle links"),
    ("ui.help.nav", "后退 / 前进 / 重载", "back / forward / reload"),
    ("ui.help.scroll", "滚动 / 翻页 / 顶 / 底", "scroll / page / top / bottom"),
    ("ui.help.find", "页内查找 / 下一个 / 上一个", "find / next / prev"),
    ("ui.help.overlays", "历史 / 命令面板 / AI / 链接", "history / palette / AI / links"),
    ("ui.help.command", "命令行（原 CLI 参数）", "command line (original CLI args)"),
    ("ui.help.assist", "人工接力 / 粘贴 cookie", "human assist / paste cookies"),
    ("ui.help.tabs", "新标签 / 关标签 / 切换", "new tab / close tab / switch"),
    ("ui.help.views", "原始文本 / markdown 视图", "raw text / markdown view"),
    ("ui.help.export", "导出 md / 导出单文件 html", "export md / export single-file html"),
    ("ui.help.quit", "复制 URL / 帮助 / 退出", "copy URL / help / quit"),
    ("ui.search", "搜索", "search"),
    ("ui.status.cookie_imported", "已导入 cookie", "cookies imported"),
    ("ui.status.cookie_no_url", "没有可用的 URL，无法导入 cookie", "no url, cannot import cookies"),
    ("ui.status.command", "命令", "command"),
    ("ui.status.unknown", "未知", "unknown"),
    ("ui.status.nothing_loaded", "还没有加载页面", "nothing loaded"),
    ("ui.status.find_none", "没有匹配", "no find matches"),
    ("ui.status.reload", "重载", "reload"),
    ("ui.status.exported_path", "已导出", "exported"),
    ("ui.status.no_browser", "没有找到 Chromium 浏览器（Edge/Chrome），可用 c 粘贴 Cookie", "no Chromium browser (Edge/Chrome) found; press c to paste cookies"),
];

pub fn t(key: &str) -> &'static str {
    let en = current() == Lang::En;
    for (k, zh, english) in CATALOG {
        if *k == key {
            return if en { english } else { zh };
        }
    }
    // 缺词条时回落到 key 本身，方便发现漏翻。
    Box::leak(key.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_language_tags() {
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("en-US"), Some(Lang::En));
        assert_eq!(Lang::parse("english"), Some(Lang::En));
        assert_eq!(Lang::parse("klingon"), None);
    }

    #[test]
    fn catalog_has_both_translations_and_unique_keys() {
        let mut seen = std::collections::HashSet::new();
        for (key, zh, en) in CATALOG {
            assert!(seen.insert(*key), "duplicate key {key}");
            assert!(!zh.trim().is_empty(), "missing zh for {key}");
            assert!(!en.trim().is_empty(), "missing en for {key}");
            // 少数词条两种语言本来就一样（比如提示符），允许白名单。
            const SAME_IN_BOTH: &[&str] = &["shell.prompt", "ui.mode.cookie", "ui.mode.ai"];
            if !SAME_IN_BOTH.contains(key) {
                assert_ne!(zh, en, "translation not filled for {key}");
            }
        }
    }

    #[test]
    fn switching_language_changes_lookup() {
        set(Lang::Zh);
        assert_eq!(t("shell.goodbye"), "再见。");
        set(Lang::En);
        assert_eq!(t("shell.goodbye"), "Bye.");
        set(Lang::Zh);
    }
}
