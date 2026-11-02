//! `cb export`: export a page as single-file HTML or markdown / 导出为单文件 HTML 或 markdown。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// 目标 URL / Target URL
    pub url: String,

    /// 输出格式 · Output format: html | md
    #[arg(long, default_value = "md", value_name = "FMT")]
    pub to: String,

    /// 写入文件（可与 --json 并存）· Write to a file (may be combined with --json)
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,

    /// 跳过缓存 · Bypass the cache
    #[arg(long)]
    pub fresh: bool,

    /// 从 cassette 回放 · Replay a recorded cassette
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,
}
