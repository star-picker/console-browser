use assert_cmd::Command;
use serde_json::Value;

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

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
fn github_engine_returns_structured_results() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("gh.cassette.json");
    let (code, v) = run(
        &dir,
        &[
            "search", "ripgrep", "--engine", "gh", "--limit", "5", "--replay", &cassette,
        ],
    );
    assert_eq!(code, 0, "{}", v);
    assert_eq!(v["data"]["engine"], "gh");
    let results = v["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["title"], "BurntSushi/ripgrep");
    assert_eq!(results[0]["url"], "https://github.com/BurntSushi/ripgrep");
    assert!(results[0]["snippet"]
        .as_str()
        .expect("snippet")
        .contains("51000"));
}

#[test]
fn ddg_engine_parses_serp_and_unwraps_redirects() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("ddg.cassette.json");
    let (code, v) = run(
        &dir,
        &[
            "search",
            "rust http",
            "--engine",
            "ddg",
            "--replay",
            &cassette,
        ],
    );
    assert_eq!(code, 0, "{}", v);
    let results = v["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["url"], "https://example.com/one");
    assert_eq!(results[1]["url"], "https://example.com/two");
}

#[test]
fn search_records_history() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("gh.cassette.json");
    let _ = run(
        &dir,
        &[
            "search", "ripgrep", "--engine", "gh", "--limit", "5", "--replay", &cassette,
        ],
    );
    let (code, v) = run(&dir, &["history", "--query", "ripgrep"]);
    assert_eq!(code, 0);
    let entries = v["data"]["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["kind"], "search");
}

#[test]
fn unknown_engine_is_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("gh.cassette.json");
    let (code, _v) = run(
        &dir,
        &[
            "search",
            "x",
            "--engine",
            "altavista",
            "--replay",
            &cassette,
        ],
    );
    assert_eq!(code, 2);
}
#[test]
fn broken_serp_is_parse_failed_not_success() {
    // P0-1: 页面拿到了、解析出 0 条，绝不能报 ok:true。
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("serp_broken.cassette.json");
    let (code, v) = run(
        &dir,
        &[
            "search",
            "rust async",
            "--engine",
            "bing",
            "--limit",
            "5",
            "--replay",
            &cassette,
        ],
    );
    assert_eq!(code, 10, "parse_failed must not exit 0: {v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "parse_failed", "{v}");
}

#[test]
fn empty_serp_reports_its_reason_and_evidence() {
    // P0-3: 零结果要能自证：empty_reason + http_status + bytes。
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("serp_shell.cassette.json");
    let (code, v) = run(
        &dir,
        &[
            "search",
            "rust async",
            "--engine",
            "bing",
            "--limit",
            "5",
            "--replay",
            &cassette,
        ],
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(
        v["data"]["results"].as_array().map(|a| a.len()),
        Some(0),
        "{v}"
    );
    assert_eq!(v["data"]["empty_reason"], "blocked", "{v}");
    assert_eq!(v["data"]["http_status"], 200, "{v}");
    assert!(v["data"]["bytes"].as_u64().unwrap_or(0) > 0, "{v}");
}

#[test]
fn arxiv_429_is_rate_limited_not_challenge() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("arxiv_429.cassette.json");
    let (code, v) = run(
        &dir,
        &[
            "search",
            "diffusion",
            "--engine",
            "arxiv",
            "--replay",
            &cassette,
        ],
    );
    assert_eq!(code, 3, "rate limited exits as network: {v}");
    assert_eq!(v["error"]["code"], "rate_limited", "{v}");
    assert_eq!(v["error"]["retryable"], true, "{v}");
    assert_eq!(v["error"]["retry_after_s"], 30, "{v}");
}
