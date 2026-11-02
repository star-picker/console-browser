//! `cb agent`: 最小自主循环，模型出动作、浏览器执行 / minimal autonomous loop.

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub cmd: AgentCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum AgentCmd {
    /// 给一个目标，让 agent 自己搜/抓/答 · Give a goal; the agent searches, fetches and answers
    #[command(
        after_help = "EXAMPLES / 例子:\n  cb agent run \"ripgrep 的作者是谁\"\n  cb agent run \"…\" --max-steps 4 --budget 2000 --json"
    )]
    Run {
        /// 目标 · Goal
        goal: String,

        /// 最多几步 · Max steps
        #[arg(long, default_value_t = 6)]
        max_steps: usize,

        /// 本任务 completion token 上限（0 = 不限）· Token cap for this run
        #[arg(long, default_value_t = 0)]
        budget: i64,

        /// 单步观察保留字符数 · Observation characters kept per step
        #[arg(long, default_value_t = 1200)]
        observation_chars: usize,

        /// 从 cassette 回放 · Replay cassettes
        #[arg(long, value_name = "FILE")]
        replay: Option<PathBuf>,
    },
}
