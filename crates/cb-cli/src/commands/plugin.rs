//! `cb plugin`: 插件清单与权限（JS 钩子等 cb-js）。

use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub cmd: PluginCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum PluginCmd {
    /// 列出已安装插件 · List installed plugins
    List {
        /// 插件目录（默认用户配置目录）· Plugin directory
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// 查看单个插件 · Show one plugin
    Info {
        /// 插件名 · Plugin name
        name: String,
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// 安装插件（整目录复制，先校验 manifest）· Install a plugin directory
    Add {
        /// 插件目录（含 plugin.json）· Directory containing plugin.json
        path: PathBuf,
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },

    /// 打印插件目录 · Print the plugin directory
    Dir,
}
