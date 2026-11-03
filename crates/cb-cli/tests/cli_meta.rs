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
fn meta_cache_is_an_object_and_extract_reports_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/page.cassette.json")
        .to_string_lossy()
        .to_string();
    let (code, v) = run(
        &dir,
        &["fetch", "https://example.com/a", "--replay", &cassette],
    );
    assert_eq!(code, 0, "{v}");
    // P1: cached 布尔之外要有对象；extract 要有字符/段落统计。
    assert!(v["meta"]["cache"]["hit"].is_boolean(), "{v}");
    assert!(
        v["meta"]["extract"]["chars"].as_u64().unwrap_or(0) > 10,
        "{v}"
    );
    assert!(
        v["meta"]["extract"]["paragraphs"].as_u64().unwrap_or(0) >= 1,
        "{v}"
    );
}
