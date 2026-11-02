//! `cb export`：把页面导出成单文件 HTML 或 markdown。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// 目标 URL
    pub url: String,

    /// 输出格式：html | md
    #[arg(long, default_value = "md", value_name = "FMT")]
    pub to: String,

    /// 写入文件（不给则打印到 stdout）
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,

    /// 跳过缓存
    #[arg(long)]
    pub fresh: bool,

    /// 从 cassette 回放
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,
}
