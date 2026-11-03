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

fn run(dir: &tempfile::TempDir, url: &str, n: &str, cassette: &str) -> (i32, Value) {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .args(["bench", url, "--n", n, "--replay", cassette]);
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn bench_success_keeps_every_sample() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, v) = run(
        &dir,
        "https://example.com/a",
        "2",
        &fixture("page.cassette.json"),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let samples = v["data"]["samples"].as_array().expect("samples");
    assert_eq!(samples.len(), 2);
    assert!(samples.iter().all(|s| s["ok"] == true));
    assert_eq!(v["data"]["completed"], 2);
    assert_eq!(v["data"]["planned"], 2);
    assert!(v["data"]["p50_ms"].as_u64().is_some(), "{v}");
    assert!(v["data"]["error"].is_null(), "{v}");
}

#[test]
fn bench_failure_returns_completed_samples() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, v) = run(
        &dir,
        "https://example.com/bench",
        "2",
        &fixture("bench_partial.cassette.json"),
    );
    assert_eq!(code, 7, "timeout exit code: {v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["data"]["samples"].as_array().expect("samples").len(), 2);
    assert_eq!(v["data"]["samples"][0]["ok"], true, "{v}");
    assert_eq!(v["data"]["samples"][1]["ok"], false, "{v}");
    assert!(v["data"]["samples"][0]["ms"].as_u64().is_some(), "{v}");
    assert_eq!(v["data"]["completed"], 2, "{v}");
    assert_eq!(v["data"]["planned"], 2, "{v}");
    assert!(
        v["data"]["p50_ms"].as_u64().is_some(),
        "successful samples must still be summarised: {v}"
    );
    let message = v["data"]["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("aborted at sample 1"),
        "error field must say where it aborted: {message}"
    );
    assert_eq!(v["error"]["code"], "timeout", "{v}");
}

#[test]
fn bench_first_sample_failure_keeps_one_sample() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, v) = run(
        &dir,
        "https://example.com/bench",
        "2",
        &fixture("bench_fail.cassette.json"),
    );
    assert_eq!(code, 7, "{v}");
    assert_eq!(v["data"]["samples"].as_array().expect("samples").len(), 1);
    assert_eq!(v["data"]["completed"], 1, "{v}");
    assert_eq!(v["data"]["samples"][0]["ok"], false, "{v}");
}
