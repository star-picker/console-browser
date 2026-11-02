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
