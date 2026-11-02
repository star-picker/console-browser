//! cb tui：强制进入全屏浏览器界面 / force the full-screen browser.

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct TuiArgs {
    /// 启动时直接打开的 URL · URL to open on start
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// 关闭鼠标（键位仍然完整）· Disable mouse support
    #[arg(long)]
    pub no_mouse: bool,

    /// 不恢复上次的标签页 · Do not restore previous tabs
    #[arg(long)]
    pub no_restore: bool,

    /// 从 cassette 回放 · Replay cassettes instead of the network
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// 纯 ASCII 符号（终端不支持线框时）· Plain ASCII glyphs
    #[arg(long)]
    pub ascii: bool,

    /// 关闭颜色 · Disable colors
    #[arg(long)]
    pub no_color: bool,
}
