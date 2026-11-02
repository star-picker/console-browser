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

/// 默认封装：Windows 用 DPAPI，其它平台退回明文标记。
pub fn default_secret_box() -> Arc<dyn SecretBox> {
    #[cfg(windows)]
    {
        Arc::new(DpapiBox)
    }
    #[cfg(not(windows))]
    {
        Arc::new(PlainBox)
    }
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
/// Windows DPAPI 封装：同一用户才能解开，跨机器/跨用户不可读。
#[cfg(windows)]
pub struct DpapiBox;

#[cfg(windows)]
const DPAPI_TAG: &[u8] = b"dp1:";

#[cfg(windows)]
impl SecretBox for DpapiBox {
    fn seal(&self, plain: &[u8]) -> Vec<u8> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        unsafe {
            let input = CRYPT_INTEGER_BLOB {
                cbData: plain.len() as u32,
                pbData: plain.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            let ok = CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            );
            if ok == 0 || output.pbData.is_null() {
                // 失败时退回明文标记，保证不丢数据（open 端两种都能读）。
                return PlainBox.seal(plain);
            }
            let mut sealed = Vec::with_capacity(DPAPI_TAG.len() + output.cbData as usize);
            sealed.extend_from_slice(DPAPI_TAG);
            sealed.extend_from_slice(std::slice::from_raw_parts(
                output.pbData,
                output.cbData as usize,
            ));
            LocalFree(output.pbData as *mut core::ffi::c_void);
            sealed
        }
    }

    fn open(&self, sealed: &[u8]) -> Vec<u8> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let Some(payload) = sealed.strip_prefix(DPAPI_TAG) else {
            return PlainBox.open(sealed);
        };
        unsafe {
            let input = CRYPT_INTEGER_BLOB {
                cbData: payload.len() as u32,
                pbData: payload.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            let ok = CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            );
            if ok == 0 || output.pbData.is_null() {
                return Vec::new();
            }
            let plain = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData as *mut core::ffi::c_void);
            plain
        }
    }

    fn name(&self) -> &'static str {
        "dpapi"
    }
}

#[cfg(windows)]
#[cfg(test)]
mod dpapi_tests {
    use super::*;

    #[test]
    fn dpapi_roundtrip_hides_plaintext() {
        let boxed = DpapiBox;
        let sealed = boxed.seal(b"cookie-secret-value");
        assert!(sealed.starts_with(DPAPI_TAG), "should be dpapi-tagged");
        assert!(!String::from_utf8_lossy(&sealed).contains("cookie-secret-value"));
        assert_eq!(boxed.open(&sealed), b"cookie-secret-value".to_vec());
        assert_eq!(boxed.name(), "dpapi");
    }

    #[test]
    fn dpapi_open_accepts_legacy_plain_bytes() {
        let boxed = DpapiBox;
        let legacy = PlainBox.seal(b"legacy");
        assert_eq!(boxed.open(&legacy), b"legacy".to_vec());
    }
}
