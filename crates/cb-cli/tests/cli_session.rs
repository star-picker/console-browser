//! daemon + 命名管道 + 句柄的端到端测试（真起进程、真走传输层）。

use serde_json::Value;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_cb")
}

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

struct DaemonGuard(Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_json(dir: &tempfile::TempDir, pipe: &str, args: &[&str]) -> (i32, Value) {
    let out = Command::new(bin())
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--pipe")
        .arg(pipe)
        .arg("--json")
        .args(args)
        .output()
        .expect("run cb");
    let code = out.status.code().unwrap_or(-1);
    let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "not json ({e}) for {:?}: stdout={} stderr={}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (code, value)
}

fn start_daemon(dir: &tempfile::TempDir, pipe: &str, cassette: &str) -> DaemonGuard {
    let child = Command::new(bin())
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .args(["daemon", "--pipe", pipe, "--replay", cassette])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    let guard = DaemonGuard(child);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (code, value) = run_json(dir, pipe, &["session", "ping"]);
        if code == 0 && value["ok"] == true {
            return guard;
        }
        if Instant::now() > deadline {
            panic!("daemon did not become ready: {value}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn daemon_round_trip_with_handles_and_delta() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let pipe = format!("cb-test-{}-{}", std::process::id(), stamp);
    let cassette = fixture("page.cassette.json");
    let _daemon = start_daemon(&dir, &pipe, &cassette);

    let (code, opened) = run_json(&dir, &pipe, &["session", "open", "https://example.com/a"]);
    assert_eq!(code, 0, "{opened}");
    assert_eq!(opened["ok"], true, "{opened}");
    let data = &opened["data"];
    assert_eq!(data["handle"], "@1", "{opened}");
    assert_eq!(data["revision"], 1, "{opened}");
    assert_eq!(data["kind"], "full", "{opened}");
    assert_eq!(data["title"], "Example Page", "{opened}");
    assert!(
        data["blocks"]
            .as_array()
            .map(|b| !b.is_empty())
            .unwrap_or(false),
        "full snapshot must carry blocks: {opened}"
    );

    let (code, unchanged) = run_json(&dir, &pipe, &["session", "get", "@1", "--since", "1"]);
    assert_eq!(code, 0, "{unchanged}");
    assert_eq!(unchanged["data"]["kind"], "unchanged", "{unchanged}");
    assert!(
        unchanged["data"].get("blocks").is_none(),
        "unchanged must not resend blocks: {unchanged}"
    );

    let (code, refreshed) = run_json(&dir, &pipe, &["session", "refresh", "@1"]);
    assert_eq!(code, 0, "{refreshed}");
    assert_eq!(refreshed["data"]["revision"], 2, "{refreshed}");

    let (code, delta) = run_json(&dir, &pipe, &["session", "get", "@1", "--since", "1"]);
    assert_eq!(code, 0, "{delta}");
    assert_eq!(delta["data"]["kind"], "delta", "{delta}");
    assert!(
        delta["data"].get("blocks").is_none(),
        "delta must not resend all blocks: {delta}"
    );
    assert_eq!(
        delta["data"]["ops"].as_array().map(|ops| ops.len()),
        Some(0),
        "same page content means no ops: {delta}"
    );

    let (code, list) = run_json(&dir, &pipe, &["session", "list"]);
    assert_eq!(code, 0, "{list}");
    assert_eq!(
        list["data"]["sessions"].as_array().map(|s| s.len()),
        Some(1)
    );

    let (code, closed) = run_json(&dir, &pipe, &["session", "close", "@1"]);
    assert_eq!(code, 0, "{closed}");
    assert_eq!(closed["data"]["closed"], "@1", "{closed}");

    let (code, gone) = run_json(&dir, &pipe, &["session", "get", "@1"]);
    assert_eq!(code, 6, "closed handle must be not_found: {gone}");
    assert_eq!(gone["error"]["code"], "not_found", "{gone}");
}

#[test]
fn missing_daemon_is_a_clear_error_with_a_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pipe = format!("cb-missing-{}", std::process::id());
    let (code, value) = run_json(&dir, &pipe, &["session", "ping"]);
    assert_ne!(code, 0, "{value}");
    assert_eq!(value["error"]["code"], "daemon_unavailable", "{value}");
    let hint = value["error"]["needs_user"]["hint"]
        .as_str()
        .unwrap_or_default();
    assert!(
        hint.contains("cb daemon"),
        "hint must tell the human how to start it: {hint}"
    );
}
