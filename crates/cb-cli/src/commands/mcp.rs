//! `cb mcp`：把能力表挂成 MCP 服务端。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub cmd: McpCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum McpCmd {
    /// 以 stdio 运行 MCP 服务端（给 Hermes 等 agent 用）
    Serve {
        /// 从 cassette 回放，不联网（测试与复盘）
        #[arg(long, value_name = "FILE")]
        replay: Option<PathBuf>,
    },
}
