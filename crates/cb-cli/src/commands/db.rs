//! `cb db`：本地库的维护入口。

#[derive(Debug, clap::Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub cmd: DbCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum DbCmd {
    /// 按保留期清理缓存 / 历史 / 日志
    Prune,
    /// 各表行数
    Stats,
}
