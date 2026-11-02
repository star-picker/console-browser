use assert_cmd::Command;
use serde_json::Value;

fn cb(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
    cmd.arg("--db").arg(dir.path().join("cb.db")).arg("--json");
    cmd
}

fn cassette() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("page.cassette.json")
        .to_string_lossy()
        .to_string()
}

fn run(cmd: &mut Command) -> (i32, Value) {
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn export_html_is_self_contained() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("page.html");
    let mut cmd = cb(&dir);
    cmd.args([
        "export",
        "https://example.com/a",
        "--to",
        "html",
        "--out",
        file.to_str().expect("utf8"),
        "--replay",
        &cassette(),
    ]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{v}");
    let html = std::fs::read_to_string(&file).expect("written file");
    assert!(html.contains("<style>"), "reader css inlined");
    assert!(html.contains("Hello World"), "content present");
    assert!(!html.contains("<script"), "scripts stripped");
    assert!(v["data"]["bytes"].as_u64().unwrap() > 0);
}

#[test]
fn export_md_returns_markdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.args(["export", "https://example.com/a", "--replay", &cassette()]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0);
    let content = v["data"]["content"].as_str().expect("content");
    assert!(content.starts_with("# Hello World"), "got: {content}");
}

#[test]
fn export_unknown_format_is_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = cb(&dir);
    cmd.args([
        "export",
        "https://example.com/a",
        "--to",
        "pdf",
        "--replay",
        &cassette(),
    ]);
    let (code, _v) = run(&mut cmd);
    assert_eq!(code, 2);
}
