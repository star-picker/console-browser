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

fn run(dir: &tempfile::TempDir, cassette: &str, extra: &[&str]) -> (i32, Value) {
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .env("CB_AI_PROVIDER", "openai")
        .env("CB_AI_BASE_URL", "https://ai.test/v1")
        .env("CB_AI_MODEL", "test-model")
        .env("CB_AI_KEY", "sk-test-not-real")
        .env("CB_AI_REPLAY", cassette);
    cmd.args(extra);
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn agent_answers_in_one_step() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("ai.cassette.json");
    let (code, v) = run(&dir, &cassette, &["agent", "run", "ripgrep 作者是谁"]);
    assert_eq!(code, 0, "{}", v);
    assert_eq!(v["data"]["finished"], true);
    assert!(v["data"]["answer"]
        .as_str()
        .expect("answer")
        .contains("Andrew Gallant"));
    let steps = v["data"]["steps"].as_array().expect("steps");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["action"], "answer");
    assert!(v["data"]["completion_tokens"].as_i64().unwrap() > 0);
}

#[test]
fn agent_tokens_are_ledgered() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("ai.cassette.json");
    let (_code, _v) = run(&dir, &cassette, &["agent", "run", "ripgrep 作者是谁"]);
    let (code, v) = run(&dir, &cassette, &["ai", "usage"]);
    assert_eq!(code, 0);
    assert!(v["data"]["today_tokens"].as_i64().unwrap() >= 32, "{}", v);
}
