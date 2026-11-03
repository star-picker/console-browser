//! `cb log`: 日志文件位置与最近记录。

#[derive(Debug, clap::Args)]
pub struct LogArgs {
    #[command(subcommand)]
    pub cmd: LogCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum LogCmd {
    /// 打印日志目录 · Print the log directory
    Dir,
    /// 统计：命令次数 / 错误码 / 平均耗时 · Aggregate counts and errors
    Stats,

    /// 看最近 N 条 · Show the latest N records
    Tail {
        /// 条数 · How many lines
        #[arg(long, default_value_t = 20)]
        n: usize,
    },
}
