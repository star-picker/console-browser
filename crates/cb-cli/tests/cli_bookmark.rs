use assert_cmd::Command;
use serde_json::Value;

fn run(dir: &tempfile::TempDir, args: &[&str]) -> (i32, Value) {
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .args(args);
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn bookmark_add_list_remove_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, v) = run(
        &dir,
        &[
            "bookmark",
            "add",
            "https://example.com/a",
            "--title",
            "Example A",
        ],
    );
    assert_eq!(code, 0, "{v}");

    let (code, v) = run(&dir, &["bookmark", "list"]);
    assert_eq!(code, 0, "{v}");
    let rows = v["data"]["bookmarks"].as_array().expect("bookmarks");
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(rows[0]["url"], "https://example.com/a");
    assert_eq!(rows[0]["title"], "Example A");

    // 幂等：同 URL 再 add 不新增，只覆盖标题
    let (code, _) = run(
        &dir,
        &[
            "bookmark",
            "add",
            "https://example.com/a",
            "--title",
            "Renamed",
        ],
    );
    assert_eq!(code, 0);
    let (_, v) = run(&dir, &["bookmark", "list"]);
    let rows = v["data"]["bookmarks"].as_array().expect("bookmarks");
    assert_eq!(rows.len(), 1, "must stay one row: {v}");
    assert_eq!(rows[0]["title"], "Renamed");

    let (code, v) = run(&dir, &["bookmark", "rm", "https://example.com/a"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["removed"], true);
    let (_, v) = run(&dir, &["bookmark", "list"]);
    assert_eq!(v["data"]["bookmarks"].as_array().map(|a| a.len()), Some(0));
}
