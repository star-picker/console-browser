use assert_cmd::Command;
use std::process::{Command as StdCommand, Stdio};

fn bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("cb")
}

#[test]
fn tui_refuses_to_start_without_a_terminal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut child = StdCommand::new(bin())
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .args(["tui", "--no-mouse"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2), "expected usage exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("needs a terminal"), "stderr: {stderr}");
    // 不能往 stdout 写 alternate screen 序列（管道场景）
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\u{1b}[?1049h"),
        "stdout must be clean: {stdout}"
    );
}

#[test]
fn bare_cb_with_piped_stdin_stays_in_repl() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut child = StdCommand::new(bin())
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--lang")
        .arg("en")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "exit").expect("write");
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Interactive mode"), "repl banner: {stdout}");
}

#[test]
fn help_lists_tui_and_shell() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::cargo_bin("cb")
        .expect("bin")
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--help")
        .output()
        .expect("help");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("tui"), "tui subcommand missing: {text}");
    assert!(text.contains("shell"), "shell subcommand missing: {text}");
}
