//! `cb ai`: 模型配置、用量台账、一次性提问 / AI config, ledger and one-shot asks.

#[derive(Debug, clap::Args)]
pub struct AiArgs {
    #[command(subcommand)]
    pub cmd: AiCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum AiCmd {
    /// 显示解析到的模型配置（不打印密钥）· Show resolved config (never prints the key)
    #[command(after_help = "EXAMPLES / 例子:\n  cb ai config\n  cb --json ai config")]
    Config,

    /// 用量台账：按天汇总 token · Ledger summary per day
    #[command(after_help = "EXAMPLES / 例子:\n  cb ai usage\n  cb --json ai usage --days 7")]
    Usage {
        /// 最近几天 · How many days
        #[arg(long, default_value_t = 14)]
        days: usize,
    },

    /// 一次性提问 · One-shot ask
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb ai ask \"一句话解释 HTTP/2 多路复用\"\n  cb ai ask \"...\" --max-tokens 200 --json"
    )]
    Ask {
        /// 提示词 · Prompt
        prompt: String,

        /// 回复长度上限 · Max completion tokens
        #[arg(long, default_value_t = 800)]
        max_tokens: u64,
    },
}
