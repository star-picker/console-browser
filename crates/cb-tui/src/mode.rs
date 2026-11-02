//! 交互模式：Browse / Input / Overlay。
//!
//! 分离模式是为了让「一个键」在不同上下文里有不同含义而不互相打架：
//! 浏览时 j 是滚动，输入时 j 就是一个字母。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Input(InputKind),
    Overlay(OverlayKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// 地址栏：输入网址或搜索词。
    Omnibox,
    /// 页内查找。
    Find,
    /// 命令行（原 CLI 参数）。
    Command,
    /// 手动粘贴 cookie。
    PasteCookies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Help,
    History,
    Links,
    Palette,
    Ai,
    Trace,
}

impl Mode {
    pub fn is_browse(self) -> bool {
        matches!(self, Mode::Browse)
    }

    pub fn input_kind(self) -> Option<InputKind> {
        match self {
            Mode::Input(kind) => Some(kind),
            _ => None,
        }
    }

    pub fn overlay_kind(self) -> Option<OverlayKind> {
        match self {
            Mode::Overlay(kind) => Some(kind),
            _ => None,
        }
    }
}
