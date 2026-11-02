use assert_cmd::Command;
use serde_json::Value;

fn cb(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
    cmd.arg("--db").arg(dir.path().join("cb.db")).arg("--json");
    cmd
}

fn run_json(cmd: &mut Command) -> (i32, Value) {
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn selftest_reports_eight_checks_all_green_or_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.arg("selftest");
    let (code, v) = run_json(&mut cmd);
    assert_eq!(code, 0, "selftest must succeed: {v}");
    let checks = v["data"]["checks"].as_array().expect("checks array");
    assert_eq!(
        checks.len(),
        8,
        "M1 ships eight checks, got {}",
        checks.len()
    );
    for c in checks {
        let status = c["status"].as_str().expect("status");
        assert!(
            status == "ok" || status == "skipped",
            "check {} failed: {c}",
            c["name"]
        );
    }
    assert_eq!(v["data"]["all_ok"], true);
}

#[test]
fn tool_registry_exposes_m1_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.arg("tools");
    let (code, v) = run_json(&mut cmd);
    assert_eq!(code, 0);
    let tools = v["data"]["tools"].as_array().expect("tools");
    let names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(names.contains(&"browse_url".to_string()), "got {names:?}");
    assert!(names.contains(&"history_search".to_string()));
    assert!(names.contains(&"selftest_status".to_string()));
    for t in tools {
        assert!(
            t["input_schema"].is_object(),
            "every tool needs a schema: {t}"
        );
    }
}

#[test]
fn envelope_error_shape_is_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("broken.cassette.json");
    let mut cmd = cb(&dir);
    cmd.args([
        "fetch",
        "https://example.com/broken",
        "--replay",
        cassette.to_str().expect("utf8"),
    ]);
    let (code, v) = run_json(&mut cmd);
    assert_eq!(code, 3);
    assert_eq!(v["ok"], false);
    assert!(v["error"]["code"].is_string());
    assert!(v["error"]["message"].is_string());
    assert!(v["error"]["retryable"].is_boolean());
    assert!(v["meta"]["engine"].is_string());
}
