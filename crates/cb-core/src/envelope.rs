//! 对外 JSON 信封：`{ok, meta, data|error}`。stdout 只放这个。

use crate::error::CbError;
use serde::{Deserialize, Serialize};

pub const ENGINE: &str = concat!("cb/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub engine: String,
    pub elapsed_ms: u64,
    pub cached: bool,
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
