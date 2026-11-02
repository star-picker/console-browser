//! `cb challenge`: 反爬挑战的观测入口 / observe anti-bot challenge outcomes.

#[derive(Debug, clap::Args)]
pub struct ChallengeArgs {
    #[command(subcommand)]
    pub cmd: ChallengeCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum ChallengeCmd {
    /// 按域名/策略统计成功失败 · Win/loss counts per domain and strategy
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb challenge stats\n  cb --json challenge stats --days 7"
    )]
    Stats {
        /// 最近几天 · Window in days
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
}
