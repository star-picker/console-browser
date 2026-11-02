//! 命令面板：动作清单 + 名字到动作的映射。

use crate::action::Action;
use crate::app::OverlayItem;

pub fn items() -> Vec<OverlayItem> {
    let mut out = Vec::new();
    let mut push = |name: &str, label: &str, detail: &str| {
        out.push(OverlayItem {
            label: label.to_string(),
            detail: detail.to_string(),
            target: format!("action:{name}"),
        });
    };
    push("reload", "重新加载 / Reload", "r");
    push("back", "后退 / Back", "b");
    push("forward", "前进 / Forward", "f");
    push("omnibox", "聚焦地址栏 / Focus URL bar", "l");
    push("new-tab", "新标签 / New tab", "Ctrl-T");
    push("close-tab", "关标签 / Close tab", "Ctrl-W");
    push("find", "查找 / Find", "/");
    push("links", "链接列表 / Links", "L");
    push("history", "历史 / History", "Ctrl-H");
    push("help", "帮助 / Help", "?");
    push("ai", "AI 面板 / AI panel", "Ctrl-A");
    push("trace", "日志 / Trace", "cb:trace");
    push("raw", "原始文本 / Raw view", "v");
    push("markdown", "markdown 视图", "Ctrl-R");
    push("export-md", "导出 markdown", "Ctrl-E");
    push("export-html", "导出单文件 HTML", "Ctrl-S");
    push("assist", "人工接力 / Human assist", "e");
    push("paste-cookies", "粘贴 Cookie", "c");
    push("lang", "切换语言 / Switch language", "—");
    push("quit", "退出 / Quit", "q");
    out
}

pub fn action_for(name: &str) -> Option<Action> {
    Some(match name {
        "reload" => Action::Reload,
        "back" => Action::Back,
        "forward" => Action::Forward,
        "omnibox" => Action::FocusOmnibox,
        "new-tab" => Action::TabNew,
        "close-tab" => Action::TabClose,
        "find" => Action::FocusFind,
        "links" => Action::ToggleLinks,
        "history" => Action::ToggleHistory,
        "help" => Action::ToggleHelp,
        "ai" => Action::ToggleAi,
        "trace" => Action::ToggleTrace,
        "raw" => Action::ToggleRawView,
        "markdown" => Action::ToggleMarkdownView,
        "export-md" => Action::ExportMarkdown,
        "export-html" => Action::ExportHtml,
        "assist" => Action::OpenHumanAssist,
        "paste-cookies" => Action::PasteCookies,
        "lang" => Action::SwitchLang,
        "quit" => Action::Quit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_maps_to_an_action() {
        let items = items();
        assert!(items.len() >= 15);
        for item in items {
            let name = item.target.strip_prefix("action:").expect("action target");
            assert!(action_for(name).is_some(), "missing action for {name}");
        }
    }

    #[test]
    fn unknown_action_is_rejected() {
        assert!(action_for("nope").is_none());
    }
}
