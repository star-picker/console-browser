//! cb-core: 契约（错误、信封、工具表）+ 页面获取管道。
//!
//! CbError 故意做成富值类型（带结构化 ErrorBody，直接落进 JSON 信封），
//! 不套 Box：多一次分配换更厚的错误信息，这里划算。
#![allow(clippy::result_large_err)]

pub mod agent;
pub mod ai;
pub mod challenge;
pub mod envelope;
pub mod error;
pub mod export;
pub mod fetch;
pub mod i18n;
pub mod login;
pub mod search;
pub mod selftest;
pub mod tools;
pub mod version;

use cb_net::Profile;
use cb_store::Store;

pub use agent::{run as run_agent, AgentOpts, AgentRun};
pub use ai::{complete, AiConfig, Completion, Provider};
pub use envelope::{CacheMeta, Envelope, ExtractMeta, Meta, ENGINE};
pub use error::CbError;
pub use fetch::{fetch_page, FetchOpts, FetchOutcome, Format};
pub use login::{detect_login_wall, LoginWall, LoginWallKind};
pub use search::{
    search, search_report, Attempt, EmptyReason, Engine, SearchOpts, SearchReport, SearchResult,
};
pub use selftest::{run_selftest, SelftestReport};
pub use tools::{registry, ToolSpec};

/// 一次命令/一次工具调用的运行上下文。
#[derive(Clone)]
pub struct Ctx {
    pub store: Store,
    pub profile: Profile,
    /// cookie jar：登录态复用（sqlite 后端）。
    pub cookies: std::sync::Arc<dyn cb_net::cookies::CookieStore>,
}

impl Ctx {
    pub fn new(store: Store, profile: Profile) -> Ctx {
        let jar = cb_store::StoreCookieJar::new(store.clone(), cb_store::default_secret_box());
        Ctx {
            store,
            profile,
            cookies: std::sync::Arc::new(jar),
        }
    }

    /// 当前 Unix 秒。测试可用 `ctx.now` 的固定时钟覆盖（见 `with_now`）。
    pub fn now(&self) -> i64 {
        now_secs()
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
