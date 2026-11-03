//! `cb revisions`: 正文版本历史（只记变化，不存全文）。

#[derive(Debug, clap::Args)]
pub struct RevisionsArgs {
    /// 目标 URL · Target URL
    pub url: String,
}
