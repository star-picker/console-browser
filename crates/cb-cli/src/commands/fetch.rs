//! `cb fetch`: fetch one page and print it / 取一个页面并输出。

use cb_core::fetch::Format;
use cb_core::FetchOpts;
use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct FetchArgs {
    /// 目标 URL / Target URL
    pub url: String,

    /// 输出格式 · Output format: md | html | text | json
    #[arg(long, default_value = "md", value_name = "FMT")]
    pub to: String,

    /// markdown token 预算（超出裁剪并标记 truncated）· Token budget for markdown
    #[arg(long, value_name = "N")]
    pub max_tokens: Option<usize>,

    /// 只提取该 CSS 选择器命中的区块 · Restrict extraction to a CSS selector
    #[arg(long, value_name = "SEL")]
    pub scope: Option<String>,

    /// 跳过缓存，强制重新取 · Bypass the cache and refetch
    #[arg(long)]
    pub fresh: bool,

    /// TLS 指纹 · TLS fingerprint profile: chrome | firefox | safari
    #[arg(long, default_value = "chrome", value_name = "P")]
    pub profile: String,

    /// 不执行 JS（M1 恒为真）· Do not run JS (always true before M3)
    #[arg(long)]
    pub no_js: bool,

    /// 从 cassette 回放，不联网 · Replay a recorded cassette instead of the network
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// 录制响应到 cassette · Record the response into a cassette
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,

    /// 写入文件（与 --json 互斥）· Write to a file (mutually exclusive with --json)
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,

    /// 超时毫秒 · Timeout in milliseconds
    #[arg(long, default_value_t = 30_000, value_name = "MS")]
    pub timeout_ms: u64,

    /// 挑战升到最后一阶时问模型（花 token）· Ask the model as the last escalation rung
    #[arg(long)]
    pub ai_escalate: bool,
}

impl FetchArgs {
    pub fn to_opts(&self) -> anyhow::Result<FetchOpts> {
        let format = Format::parse(&self.to).ok_or_else(|| {
            anyhow::anyhow!("unknown --to value: {} (md|html|text|json)", self.to)
        })?;
        Ok(FetchOpts {
            url: self.url.clone(),
            format,
            max_tokens: self.max_tokens,
            scope: self.scope.clone(),
            fresh: self.fresh,
            no_js: self.no_js,
            replay: self.replay.clone(),
            record: self.record.clone(),
            timeout_ms: self.timeout_ms,
            ai_escalate: self.ai_escalate,
        })
    }
}
