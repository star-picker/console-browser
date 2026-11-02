//! 统一错误 + 退出码。退出码表是对外契约，测试依赖它。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedsUser {
    pub kind: String,
    pub url: Option<String>,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub needs_user: Option<NeedsUser>,
}

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}", code = .body.code, message = .body.message)]
pub struct CbError {
    pub body: ErrorBody,
}

pub const EXIT_OK: i32 = 0;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_NETWORK: i32 = 3;
pub const EXIT_CHALLENGE: i32 = 4;
pub const EXIT_JS: i32 = 5;
pub const EXIT_NOT_FOUND: i32 = 6;
pub const EXIT_TIMEOUT: i32 = 7;
pub const EXIT_NEEDS_AUTH: i32 = 8;
pub const EXIT_BUDGET: i32 = 9;
pub const EXIT_OTHER: i32 = 1;

impl CbError {
    fn new(code: &str, message: impl Into<String>, retryable: bool) -> CbError {
        CbError {
            body: ErrorBody {
                code: code.to_string(),
                message: message.into(),
                retryable,
                needs_user: None,
            },
        }
    }

    pub fn usage(message: impl Into<String>) -> CbError {
        CbError::new("usage", message, false)
    }

    pub fn network(message: impl Into<String>) -> CbError {
        CbError::new("network", message, true)
    }

    pub fn challenge(message: impl Into<String>, url: &str, hint: &str) -> CbError {
        let mut err = CbError::new("challenge_unsolved", message, false);
        err.body.needs_user = Some(NeedsUser {
            kind: "challenge".to_string(),
            url: Some(url.to_string()),
            hint: hint.to_string(),
        });
        err
    }

    pub fn js(message: impl Into<String>) -> CbError {
        CbError::new("js_error", message, false)
    }

    pub fn not_found(message: impl Into<String>) -> CbError {
        CbError::new("not_found", message, false)
    }

    pub fn timeout(message: impl Into<String>) -> CbError {
        CbError::new("timeout", message, true)
    }

    pub fn needs_auth(message: impl Into<String>, url: &str) -> CbError {
        let mut err = CbError::new("needs_auth", message, false);
        err.body.needs_user = Some(NeedsUser {
            kind: "credentials".to_string(),
            url: Some(url.to_string()),
            hint: "provide cookies or credentials, then retry".to_string(),
        });
        err
    }

    pub fn budget(message: impl Into<String>) -> CbError {
        CbError::new("budget_exhausted", message, false)
    }

    pub fn other(message: impl Into<String>) -> CbError {
        CbError::new("error", message, false)
    }

    pub fn exit_code(&self) -> i32 {
        match self.body.code.as_str() {
            "usage" => EXIT_USAGE,
            "network" | "not_recorded" => EXIT_NETWORK,
            "challenge_unsolved" => EXIT_CHALLENGE,
            "js_error" => EXIT_JS,
            "not_found" => EXIT_NOT_FOUND,
            "timeout" => EXIT_TIMEOUT,
            "needs_auth" => EXIT_NEEDS_AUTH,
            "budget_exhausted" => EXIT_BUDGET,
            _ => EXIT_OTHER,
        }
    }
}

impl From<anyhow::Error> for CbError {
    fn from(e: anyhow::Error) -> Self {
        CbError::other(e.to_string())
    }
}

impl From<cb_net::NetError> for CbError {
    fn from(e: cb_net::NetError) -> Self {
        match &e {
            cb_net::NetError::NotRecorded(url) => CbError::new(
                "not_recorded",
                format!("url not recorded in cassette: {url}"),
                false,
            ),
            cb_net::NetError::Http(status) => CbError::network(format!("http status {status}")),
            _ => CbError::network(e.to_string()),
        }
    }
}

/// 存储错误统一包装（cb-store 用 anyhow，不引入独立错误类型）。
pub fn store_err(e: anyhow::Error) -> CbError {
    CbError::other(format!("store: {e}"))
}
