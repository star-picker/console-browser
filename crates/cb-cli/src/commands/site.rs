//! `cb site`: 学到的站点策略（哪个指纹在这家站能过）。

#[derive(Debug, clap::Args)]
pub struct SiteArgs {
    #[command(subcommand)]
    pub cmd: SiteCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum SiteCmd {
    /// 列出学到的站点 · List learned site strategies
    List,
}
