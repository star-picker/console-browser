use assert_cmd::Command;
use std::io::Write;
use std::process::{Command as StdCommand, Stdio};

fn bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("cb")
}

fn run_shell(dir: &tempfile::TempDir, lines: &[&str]) -> (i32, String) {
    let mut child = StdCommand::new(bin())
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--lang")
        .arg("en")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CB_LANG", "en")
        .spawn()
        .expect("spawn shell");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for line in lines {
            writeln!(stdin, "{line}").expect("write");
        }
        stdin.flush().expect("flush");
    }
    let out = child.wait_with_output().expect("wait");
    let code = out.status.code().unwrap_or(-1);
    (code, String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn double_click_opens_interactive_shell() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run_shell(&dir, &["help", "exit"]);
    assert_eq!(code, 0, "shell must exit clean: {stdout}");
    // 非 TTY 下 rustyline 不打印提示符，靠 banner 判断进入了交互模式。
    assert!(
        stdout.contains("Interactive mode"),
        "banner expected: {stdout}"
    );
    assert!(
        stdout.contains("Commands:"),
        "help listing expected: {stdout}"
    );
    assert!(
        stdout.contains("Bye."),
        "english goodbye expected: {stdout}"
    );
}

#[test]
fn shell_runs_subcommands_without_restarting() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run_shell(&dir, &["tools", "db stats", "exit"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("browse_url"), "tools output: {stdout}");
    assert!(stdout.contains("pages"), "db stats output: {stdout}");
    assert!(!stdout.contains("unrecognized subcommand"), "{stdout}");
}

#[test]
fn lang_switch_changes_shell_language() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run_shell(&dir, &["lang zh", "exit"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("再见"),
        "chinese goodbye expected: {stdout}"
    );
}

#[test]
fn per_command_help_follows_selected_language() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::cargo_bin("cb")
        .expect("bin")
        .args(["--db", dir.path().join("cb.db").to_str().expect("utf8")])
        .args(["fetch", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out).to_string();
    // 默认（中文环境）帮助只显示一种语言，不再中英拼接。
    assert!(
        text.contains("取页面并输出") || text.contains("Fetch a page"),
        "about missing: {text}"
    );
    assert!(text.contains("--max-tokens"), "flags listed: {text}");
    assert!(
        !(text.contains("取页面并输出") && text.contains("Fetch a page")),
        "help must be single language: {text}"
    );
}

#[test]
fn db_schema_lists_core_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::cargo_bin("cb")
        .expect("bin")
        .args(["--db", dir.path().join("cb.db").to_str().expect("utf8")])
        .args(["--json", "db", "schema"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("json");
    let names: Vec<String> = v["data"]["tables"]
        .as_array()
        .expect("tables")
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    for expected in ["pages", "cache_entries", "history", "logs", "settings"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing {expected}: {names:?}"
        );
    }
}
