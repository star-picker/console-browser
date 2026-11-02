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
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db").arg(dir.path().join("cb.db")).arg("--json");
    cmd
}

fn run(cmd: &mut Command) -> (i32, Value) {
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn blocked_page_walks_the_fingerprint_ladder_and_is_recorded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("blocked.cassette.json");
    let mut fetch = cb(&dir);
    fetch.args(["fetch", "https://blocked.example/x", "--replay", &cassette]);
    let (code, v) = run(&mut fetch);
    assert_eq!(code, 4, "403 must exit with the challenge code: {v}");
    assert_eq!(v["error"]["code"], "challenge_unsolved");
    assert_eq!(v["error"]["needs_user"]["kind"], "challenge");

    let mut stats = cb(&dir);
    stats.args(["challenge", "stats"]);
    let (code, v) = run(&mut stats);
    assert_eq!(code, 0);
    let entries = v["data"]["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 3, "one event per ladder attempt: {v}");
    assert!(entries.iter().all(|e| e["domain"] == "blocked.example"));
    assert!(entries.iter().all(|e| e["losses"] == 1));
    assert!(entries
        .iter()
        .any(|e| e["strategy"].as_str().unwrap_or("").contains("chrome")));
}
