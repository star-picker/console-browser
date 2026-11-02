use assert_cmd::Command;
use serde_json::Value;
use std::io::Write;
use std::process::{Command as StdCommand, Stdio};

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
fn manual_login_imports_pasted_cookies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("cb"))
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .args(["login", "https://a.example/login", "--manual"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "sid=abc; theme=dark").expect("write");
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["data"]["saved"], 2, "{}", v);
    assert_eq!(v["data"]["mode"], "manual");

    let mut list = cb(&dir);
    list.args(["cookies", "list", "--domain", "a.example"]);
    let (code, v) = run(&mut list);
    assert_eq!(code, 0);
    assert_eq!(v["data"]["count"], 2, "{}", v);
}

#[test]
fn manual_login_rejects_garbage_stdin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("cb"))
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .args(["login", "https://a.example/", "--manual"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "this is not a cookie header").expect("write");
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2), "usage error expected");
}

#[test]
fn plain_output_strips_markdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let mut cmd = cb(&dir);
    cmd.args(["plain", "https://example.com/a", "--replay", &cassette]);
    let (code, v) = run(&mut cmd);
    assert_eq!(code, 0, "{}", v);
    let content = v["data"]["content"].as_str().expect("content");
    assert!(content.contains("Hello World"), "{}", content);
    assert!(
        !content.contains("# Hello"),
        "markdown markers must be gone: {}",
        content
    );
}
