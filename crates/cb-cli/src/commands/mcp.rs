//! `cb mcp`: expose the tool registry over MCP / 把能力表挂成 MCP 服务端。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub cmd: McpCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum McpCmd {
    /// 以 stdio 运行 MCP 服务端 · Serve MCP over stdio (for Hermes and other agents)
    #[command(
        after_help = "EXAMPLES:\n  cb mcp serve\n  cb mcp serve --replay tests/fixtures/page.cassette.json"
    )]
    Serve {
        /// 从 cassette 回放，不联网 · Replay cassettes instead of the network
        #[arg(long, value_name = "FILE")]
        replay: Option<PathBuf>,
    },
}
