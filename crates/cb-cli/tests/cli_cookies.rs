use assert_cmd::Command;
use serde_json::Value;

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
fn cookies_start_empty_and_clear_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut list = cb(&dir);
    list.args(["cookies", "list"]);
    let (code, v) = run(&mut list);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["count"], 0);

    let mut clear = cb(&dir);
    clear.args(["cookies", "clear"]);
    let (code, v) = run(&mut clear);
    assert_eq!(code, 0);
    assert_eq!(v["data"]["cleared"], true);
}

#[test]
fn cookies_list_accepts_domain_filter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut list = cb(&dir);
    list.args(["cookies", "list", "--domain", "example.com"]);
    let (code, v) = run(&mut list);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["count"], 0);
}
