//! 自检：8 项机器可读检查，是外部自动化接入的统一入口。

use crate::Ctx;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: &'static str,
    /// ok | fail | skipped
    pub status: &'static str,
    pub detail: String,
}

impl CheckResult {
    fn ok(name: &'static str, detail: impl Into<String>) -> CheckResult {
        CheckResult {
            name,
            status: "ok",
            detail: detail.into(),
        }
    }

    fn skipped(name: &'static str, detail: impl Into<String>) -> CheckResult {
        CheckResult {
            name,
            status: "skipped",
            detail: detail.into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> CheckResult {
        CheckResult {
            name,
            status: "fail",
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusCounts {
    pub ok: usize,
    pub skipped: usize,
    pub fail: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelftestReport {
    pub all_ok: bool,
    /// 细分状态：ok（全部通过）/ degraded（有跳过项，功能可用但不完整）/ fail。
    pub status: &'static str,
    pub counts: StatusCounts,
    pub checks: Vec<CheckResult>,
}

pub fn run_selftest(ctx: &Ctx) -> SelftestReport {
    let mut checks = Vec::with_capacity(8);

    match ctx.store.schema_version() {
        Ok(v) => checks.push(CheckResult::ok("db", format!("schema version {v}"))),
        Err(e) => checks.push(CheckResult::fail("db", e.to_string())),
    }

    let probe = std::env::temp_dir().join(format!("cb-selftest-{}.tmp", std::process::id()));
    match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
        Ok(()) => checks.push(CheckResult::ok("fs", "temp dir writable")),
        Err(e) => checks.push(CheckResult::fail("fs", e.to_string())),
    }

    match cb_net::NetClient::new(ctx.profile) {
        Ok(_) => checks.push(CheckResult::ok(
            "net",
            format!("client builds (profile {})", ctx.profile.as_str()),
        )),
        Err(e) => checks.push(CheckResult::fail("net", e.to_string())),
    }

    checks.push(CheckResult::skipped(
        "tls",
        "oracle check needs network: cargo test -p cb-net --test tls_profile -- --ignored",
    ));
    checks.push(CheckResult::skipped("js", "JS engine lands in M3"));
    checks.push(CheckResult::skipped(
        "llm",
        "no LLM configured (M2); cb runs fully local without one",
    ));

    match cb_store::Store::open_in_memory() {
        Ok(store) => {
            let entry = cb_store::CacheEntry {
                key: "selftest".to_string(),
                method: "GET".to_string(),
                url: "https://example.invalid/".to_string(),
                status: 200,
                headers: "{}".to_string(),
                body: b"ok".to_vec(),
                expires_at: crate::now_secs() + 60,
                validators: "{}".to_string(),
                created_at: crate::now_secs(),
            };
            match store
                .cache_put(&entry)
                .and_then(|_| store.cache_get("selftest"))
            {
                Ok(Some(found)) if found.body == b"ok" => {
                    checks.push(CheckResult::ok("cache", "put/get roundtrip in memory db"))
                }
                Ok(_) => checks.push(CheckResult::fail("cache", "roundtrip returned nothing")),
                Err(e) => checks.push(CheckResult::fail("cache", e.to_string())),
            }
        }
        Err(e) => checks.push(CheckResult::fail("cache", e.to_string())),
    }

    let tools = crate::tools::registry();
    let serialized = serde_json::to_string(tools).unwrap_or_default();
    if tools.len() >= 3 && serialized.contains("browse_url") {
        checks.push(CheckResult::ok(
            "mcp",
            format!("{} tools serialize for MCP", tools.len()),
        ));
    } else {
        checks.push(CheckResult::fail("mcp", "tool registry did not serialize"));
    }

    let counts = StatusCounts {
        ok: checks.iter().filter(|c| c.status == "ok").count(),
        skipped: checks.iter().filter(|c| c.status == "skipped").count(),
        fail: checks.iter().filter(|c| c.status == "fail").count(),
    };
    let status = if counts.fail > 0 {
        "fail"
    } else if counts.skipped > 0 {
        "degraded"
    } else {
        "ok"
    };
    SelftestReport {
        all_ok: counts.fail == 0,
        status,
        counts,
        checks,
    }
}
