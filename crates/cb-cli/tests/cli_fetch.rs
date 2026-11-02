use assert_cmd::Command;
use serde_json::Value;

fn cb(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("cb").expect("bin cb");
    cmd.arg("--db").arg(dir.path().join("cb.db")).arg("--json");
    cmd
}

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn parse(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid json ({e}): {}",
            String::from_utf8_lossy(stdout)
        )
    })
}

#[test]
fn fetch_md_json_shape_and_cache_hit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");

    let out = cb(&dir)
        .args(["fetch", "https://example.com/a", "--replay", &cassette])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["meta"]["cached"], false);
    assert_eq!(v["meta"]["profile"], "chrome");
    assert_eq!(v["meta"]["engine"], "cb/0.1.0");
    let content = v["data"]["content"].as_str().expect("content string");
    assert!(content.contains("# Hello World"), "got: {content}");
    assert!(content.contains("```rust"), "code fence with language");
    assert_eq!(v["data"]["status"], 200);
    assert_eq!(v["data"]["title"], "Example Page");
    assert!(v["data"]["tokens_estimate"].as_u64().unwrap() > 0);

    let out2 = cb(&dir)
        .args(["fetch", "https://example.com/a", "--replay", &cassette])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v2 = parse(&out2);
    assert_eq!(
        v2["meta"]["cached"], true,
        "second read must be a cache hit"
    );
    assert!(v2["data"]["cache_age_s"].as_i64().is_some());
}

#[test]
fn server_error_exits_3_with_network_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("broken.cassette.json");
    let out = cb(&dir)
        .args(["fetch", "https://example.com/broken", "--replay", &cassette])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "network");
}

#[test]
fn html_format_returns_raw_markup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--to",
            "html",
            "--replay",
            &cassette,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    let content = v["data"]["content"].as_str().expect("content");
    assert!(content.contains("<h1>Hello World</h1>"));
}

#[test]
fn unknown_format_is_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--to",
            "pdf",
            "--replay",
            &cassette,
        ])
        .assert()
        .code(2);
}
