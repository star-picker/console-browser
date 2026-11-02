//! cb-tui: 全屏终端浏览器（状态机 + 视图）。
//!
//! 分层：mode/action/keymap 是纯逻辑（可测），app 管状态，views 只管画。

pub mod action;
pub mod app;
pub mod editbuffer;
pub mod keymap;
pub mod mode;
pub mod route;

pub use action::Action;
pub use app::{App, Effect, LoadStatus, OverlayItem, PageView, Tab};
pub use editbuffer::EditBuffer;
pub use keymap::key_to_action;
pub use mode::{InputKind, Mode, OverlayKind};
