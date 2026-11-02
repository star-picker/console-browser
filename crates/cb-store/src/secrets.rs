//! 敏感值的封装（cookie 落库前加密）。
//!
//! 当前发布版用 PlainBox（本机 sqlite，明文）；Windows DPAPI 版本留在 T12，
//! 接口先固定下来，换实现不需要改调用方。

use std::sync::Arc;

pub trait SecretBox: Send + Sync {
    fn seal(&self, plain: &[u8]) -> Vec<u8>;
    fn open(&self, sealed: &[u8]) -> Vec<u8>;
    /// 给 `cb selftest` / `cb cookies list` 报告用。
    fn name(&self) -> &'static str;
}

/// 明文实现：不做混淆，只在字段前面加一个标记，便于以后识别旧数据。
pub struct PlainBox;

const PLAIN_TAG: &[u8] = b"plain:";

impl SecretBox for PlainBox {
    fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(PLAIN_TAG.len() + plain.len());
        out.extend_from_slice(PLAIN_TAG);
        out.extend_from_slice(plain);
        out
    }

    fn open(&self, sealed: &[u8]) -> Vec<u8> {
        sealed.strip_prefix(PLAIN_TAG).unwrap_or(sealed).to_vec()
    }

    fn name(&self) -> &'static str {
        "plain"
    }
}

pub fn default_secret_box() -> Arc<dyn SecretBox> {
    Arc::new(PlainBox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_roundtrip() {
        let boxed = PlainBox;
        let sealed = boxed.seal(b"secret-value");
        assert_ne!(sealed, b"secret-value");
        assert_eq!(boxed.open(&sealed), b"secret-value".to_vec());
        assert_eq!(boxed.name(), "plain");
    }

    #[test]
    fn opens_untagged_legacy_bytes() {
        let boxed = PlainBox;
        assert_eq!(boxed.open(b"raw"), b"raw".to_vec());
    }
}
