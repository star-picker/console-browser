//! cb cookies：查看/清理登录态 cookie（只报名字，绝不打印值）。

#[derive(Debug, clap::Args)]
pub struct CookiesArgs {
    #[command(subcommand)]
    pub cmd: CookiesCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum CookiesCmd {
    /// 列出 cookie（名字与域，不含值）· List cookies (names only, never values)
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb cookies list\n  cb --json cookies list --domain example.com"
    )]
    List {
        /// 限定域名 · Only this domain
        #[arg(long, value_name = "D")]
        domain: Option<String>,
    },
    /// 清空 cookie · Clear cookies
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb cookies clear --domain example.com\n  cb cookies clear"
    )]
    Clear {
        /// 限定域名 · Only this domain
        #[arg(long, value_name = "D")]
        domain: Option<String>,
    },
}
