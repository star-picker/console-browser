//! `cb bookmark`: 收藏（本地 sqlite，和 history/pages 同一份库）。

#[derive(Debug, clap::Args)]
pub struct BookmarkArgs {
    #[command(subcommand)]
    pub cmd: BookmarkCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum BookmarkCmd {
    /// 添加/覆盖收藏 · Add or update a bookmark
    Add {
        /// 目标 URL · Target URL
        url: String,
        /// 标题（默认用 URL）· Title (defaults to the URL)
        #[arg(long, value_name = "T")]
        title: Option<String>,
    },
    /// 列出收藏 · List bookmarks
    List,
    /// 删除收藏 · Remove a bookmark
    Rm {
        /// 目标 URL · Target URL
        url: String,
    },
}
