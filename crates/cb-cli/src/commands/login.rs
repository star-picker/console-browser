//! cb login：人工接力登录（浏览器窗口或粘贴 Cookie）。

#[derive(Debug, clap::Args)]
pub struct LoginArgs {
    /// 要登录的 URL · Target URL
    pub url: String,

    /// 不弹浏览器，改为终端粘贴 Cookie · Paste cookies instead of opening a browser
    #[arg(long)]
    pub manual: bool,

    /// 指定浏览器 · Prefer edge or chrome
    #[arg(long, value_name = "B")]
    pub browser: Option<String>,

    /// 完事后不关助手浏览器 · Keep the assist browser open
    #[arg(long)]
    pub keep_open: bool,

    /// 等待上限秒数 · Timeout in seconds
    #[arg(long, default_value_t = 300, value_name = "S")]
    pub timeout_secs: u64,
}
