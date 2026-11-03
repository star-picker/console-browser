//! `cb search`: structured web search / 结构化搜索。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct SearchArgs {
    /// 查询词 / Query string
    pub query: String,

    /// 引擎 · Engine: auto | ddg | bing | gh | arxiv
    #[arg(long, default_value = "ddg", value_name = "E")]
    pub engine: String,

    /// 结果条数 · How many results
    #[arg(long, default_value_t = 10)]
    pub limit: usize,

    /// 从 cassette 回放 · Replay a recorded cassette
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// 超时毫秒 · Timeout in milliseconds
    #[arg(long, default_value_t = 30_000, value_name = "MS")]
    pub timeout_ms: u64,
}
