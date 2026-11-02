//! `cb db`: local database maintenance / 本地库维护。

#[derive(Debug, clap::Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub cmd: DbCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum DbCmd {
    /// 按保留期清理缓存 / 历史 / 日志 · Prune cache, history and logs by retention
    #[command(after_help = "EXAMPLES:\n  cb db prune\n  cb --json db prune")]
    Prune,

    /// 各表行数 · Row counts per table
    #[command(after_help = "EXAMPLES:\n  cb db stats\n  cb --json db stats")]
    Stats,

    /// 表结构（列 + 行数）· Table schema (columns and row counts)
    #[command(
        after_help = "EXAMPLES:\n  cb db schema\n  sqlite3 $env:LOCALAPPDATA/console-browser/cb.db .tables"
    )]
    Schema,
}
