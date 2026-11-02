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
            if *key != "shell.prompt" {
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
