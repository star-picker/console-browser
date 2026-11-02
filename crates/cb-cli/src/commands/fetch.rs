//! `cb fetch`：一次取页面并输出指定格式。

use cb_core::fetch::Format;
use cb_core::FetchOpts;
use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct FetchArgs {
    /// 目标 URL
    pub url: String,

    /// 输出格式：md | html | text | json
    #[arg(long, default_value = "md", value_name = "FMT")]
    pub to: String,

    /// markdown token 预算（超出则裁剪并标记 truncated）
    #[arg(long, value_name = "N")]
    pub max_tokens: Option<usize>,

    /// 只提取该 CSS 选择器命中的区块
    #[arg(long, value_name = "SEL")]
    pub scope: Option<String>,

    /// 跳过缓存，强制重新取
    #[arg(long)]
    pub fresh: bool,

    /// TLS 指纹 profile：chrome | firefox | safari
    #[arg(long, default_value = "chrome", value_name = "P")]
    pub profile: String,

    /// 不执行 JS（M1 恒为真，保留参数位）
    #[arg(long)]
    pub no_js: bool,

    /// 从 cassette 回放（离线测试/复现）
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// 录制响应到 cassette
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,

    /// 写入文件而不是 stdout
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,

    /// 超时毫秒
    #[arg(long, default_value_t = 30_000, value_name = "MS")]
    pub timeout_ms: u64,
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
        })
    }
}
