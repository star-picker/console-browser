//! cb js：无 URL 也有 DOM/定时器桩；事件循环能跑到定时器。

use assert_cmd::Command;
use serde_json::Value;

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn cb(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
    cmd.arg("--db").arg(dir.path().join("cb.db")).arg("--json");
    cmd
}

fn run(cmd: &mut Command) -> (i32, Value) {
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, value)
}

#[test]
fn code_without_url_still_has_document_and_timers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.args([
        "js",
        "--code",
        "typeof document + ' ' + typeof setTimeout + ' ' + typeof setInterval",
    ]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["result"], "object function function", "{v}");
}

#[test]
fn event_loop_runs_timeouts_scheduled_by_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.args([
        "js",
        "--code",
        "globalThis.__log = []; setTimeout(function(){ __log.push('x'); }, 0); __log.length",
        "--loop-ms",
        "100",
    ]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["timers_fired"], 1, "{v}");
    assert_eq!(v["data"]["result"], "0", "result is read before the loop");
}

#[test]
fn inline_scripts_plus_event_loop_observe_late_title() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("inline_timers.cassette.json");
    let mut cmd = cb(&dir);
    cmd.args([
        "js",
        "https://example.com/timers",
        "--replay",
        &cassette,
        "--run-inline",
        "--loop-ms",
        "300",
    ]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    assert!(v["data"]["timers_fired"].as_u64().unwrap_or(0) >= 1, "{v}");
    assert_eq!(
        v["data"]["title"], "Late Title",
        "the timeout must have run before reading the title: {v}"
    );
}
