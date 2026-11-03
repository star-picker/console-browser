//! 对外 JSON 信封：`{ok, meta, data|error}`。stdout 只放这个。

use crate::error::CbError;
use serde::{Deserialize, Serialize};

/// 信封里报告的引擎串，含构建日期与提交 hash。
pub const ENGINE: &str = concat!(
    "cb/",
    env!("CARGO_PKG_VERSION"),
    ".",
    env!("CB_BUILD_DATE"),
    ".",
    env!("CB_BUILD_COMMIT")
);

/// 缓存命中详情（试用报告 P1：布尔不够，调用方要能判断新鲜度）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheMeta {
    pub hit: bool,
    /// 命中缓存的年龄（秒）；未知时 None。
    pub age_s: Option<u64>,
    /// 是否命中但已过期（用旧数据兜底）。
    pub stale: bool,
}

/// 正文抽取统计（调用方判断「这个页面抽得好不好」）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractMeta {
    pub chars: usize,
    pub paragraphs: usize,
    /// 主内容容器得分（暂未接入，保留字段位）。
    pub container_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub engine: String,
    pub elapsed_ms: u64,
    /// 兼容字段：等价于 cache.hit。
    pub cached: bool,
    pub cache: CacheMeta,
    pub extract: Option<ExtractMeta>,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub ok: bool,
    pub meta: Meta,
    pub data: Option<serde_json::Value>,
    pub error: Option<crate::error::ErrorBody>,
}

impl Envelope {
    pub fn new(
        meta: Meta,
        data: Option<serde_json::Value>,
        error: Option<crate::error::ErrorBody>,
    ) -> Envelope {
        Envelope {
            ok: error.is_none(),
            meta,
            data,
            error,
        }
    }

    pub fn ok(meta: Meta, data: serde_json::Value) -> Envelope {
        Envelope::new(meta, Some(data), None)
    }

    pub fn err(meta: Meta, error: &CbError) -> Envelope {
        Envelope::new(meta, None, Some(error.body.clone()))
    }
}
