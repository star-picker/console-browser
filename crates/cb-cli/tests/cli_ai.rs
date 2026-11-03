use assert_cmd::Command;
use serde_json::Value;

fn cb(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
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
fn config_reports_provider_without_leaking_the_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.env("CB_AI_PROVIDER", "deepseek")
        .env("DEEPSEEK_API_KEY", "sk-not-a-real-key-value")
        .env("CB_AI_MODEL", "deepseek-chat")
        .arg("ai")
        .arg("config");
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["provider"], "deepseek");
    assert_eq!(v["data"]["model"], "deepseek-chat");
    assert_eq!(v["data"]["api_key_present"], true);
    let rendered = v.to_string();
    assert!(
        !rendered.contains("sk-not-a-real-key-value"),
        "api key leaked into output: {rendered}"
    );
}

#[test]
fn ask_without_key_is_a_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.env_remove("DEEPSEEK_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CB_AI_KEY")
        .env("CB_AI_PROVIDER", "deepseek")
        .arg("ai")
        .arg("ask")
        .arg("ping");
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["error"]["code"], "ai_not_configured");
    assert!(v["error"]["needs_user"]["hint"].is_string());
}

#[test]
fn usage_starts_empty_and_counts_today() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.arg("ai").arg("usage").arg("--days").arg("3");
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["today_tokens"], 0);
    assert_eq!(v["data"]["days"].as_array().expect("days").len(), 0);
}
#[test]
fn stream_prints_deltas_as_they_arrive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ai_stream.cassette.json")
        .to_string_lossy()
        .to_string();
    let out = Command::cargo_bin("cb")
        .expect("bin cb")
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .env("CB_AI_PROVIDER", "openai")
        .env("CB_AI_BASE_URL", "https://ai.test/v1")
        .env("CB_AI_MODEL", "test-model")
        .env("OPENAI_API_KEY", "sk-test-not-real")
        .env("CB_AI_REPLAY", &cassette)
        .args(["ai", "ask", "ping", "--stream"])
        .output()
        .expect("run cb");
    assert!(
        out.status.success(),
        "stream failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pong"), "streamed text missing: {stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("streamed"), "usage line missing: {stderr}");
}

#[test]
fn stream_and_json_are_mutually_exclusive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ai_stream.cassette.json")
        .to_string_lossy()
        .to_string();
    let mut cmd = cb(&dir);
    cmd.env("CB_AI_PROVIDER", "openai")
        .env("CB_AI_BASE_URL", "https://ai.test/v1")
        .env("OPENAI_API_KEY", "sk-test-not-real")
        .env("CB_AI_REPLAY", &cassette)
        .args(["ai", "ask", "ping", "--stream"]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 2, "{v}");
    assert_eq!(v["error"]["code"], "usage", "{v}");
}
