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
    assert_eq!(v["meta"]["cache"]["hit"], false);
    assert!(
        v["meta"]["cache"]["age_s"].is_null(),
        "cold fetch has no cache age: {v}"
    );
    assert_eq!(v["meta"]["profile"], "chrome");
    let engine = v["meta"]["engine"].as_str().expect("engine");
    assert!(
        engine.starts_with("cb/0.1.0."),
        "engine must carry build stamp, got {engine}"
    );
    assert_eq!(
        engine.split('.').count(),
        5,
        "xx.yy.zz.YYYYMMDD.hash shape: {engine}"
    );
    let content = v["data"]["content"].as_str().expect("content string");
    assert!(content.contains("# Hello World"), "got: {content}");
    assert!(content.contains("```rust"), "code fence with language");
    assert_eq!(v["data"]["status"], 200);
    assert_eq!(v["data"]["title"], "Example Page");
    assert!(v["data"]["tokens_estimate"].as_u64().unwrap() > 0);
    let score = v["meta"]["extract"]["container_score"]
        .as_f64()
        .expect("container_score must be a real number, not null");
    assert!((0.0..=1.0).contains(&score), "score out of range: {score}");

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
    assert_eq!(v2["meta"]["cache"]["hit"], true);
    let age = v2["meta"]["cache"]["age_s"]
        .as_u64()
        .expect("meta.cache.age_s must carry the real age");
    assert!(age <= 3, "age too large for a just-cached page: {age}");
    assert_eq!(
        Some(age as i64),
        v2["data"]["cache_age_s"].as_i64(),
        "meta and data must agree on cache age"
    );
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
#[test]
fn text_format_is_plain_not_markdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--to",
            "text",
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
    assert!(content.contains("Hello World"), "got: {content}");
    assert!(
        !content.contains("# Hello World"),
        "plain text must not keep markdown headings: {content}"
    );
    assert!(
        !content.contains("**"),
        "plain text must not keep emphasis markers: {content}"
    );
}

#[test]
fn cache_hit_preserves_redirect_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("redirect.cassette.json");

    let out = cb(&dir)
        .args(["fetch", "https://example.com/old", "--replay", &cassette])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cold = parse(&out);
    assert_eq!(cold["data"]["final_url"], "https://example.com/new");
    assert_eq!(cold["meta"]["cached"], false);

    let out2 = cb(&dir)
        .args(["fetch", "https://example.com/old", "--replay", &cassette])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let warm = parse(&out2);
    assert_eq!(warm["meta"]["cached"], true);
    assert_eq!(
        warm["data"]["final_url"], "https://example.com/new",
        "cache hit must not flip the redirect metadata"
    );
    assert_eq!(warm["data"]["bytes"], cold["data"]["bytes"]);
}

#[test]
fn out_and_json_together_is_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let out = dir.path().join("x.md");
    cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--json",
            "--out",
            out.to_str().expect("utf8"),
            "--replay",
            &cassette,
        ])
        .assert()
        .code(2);
    assert!(
        !out.exists(),
        "--out must not be written when --json wins an error"
    );
}

#[test]
fn login_wall_is_reported_not_fatal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("login.cassette.json");
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db").arg(dir.path().join("cb.db"));
    cmd.args([
        "--json",
        "fetch",
        "https://site.example/login",
        "--replay",
        &cassette,
    ]);
    let out = cmd.assert().success().get_output().clone();
    let v = parse(&out.stdout);
    assert_eq!(v["data"]["login"]["kind"], "login", "{}", v);
    assert!(!v["data"]["login"]["evidence"]
        .as_array()
        .expect("evidence")
        .is_empty());
    assert!(
        v["data"].get("doc").is_none(),
        "structured doc must not leak into json: {v}"
    );
    assert!(!v["data"]["login"]["hints"]
        .as_array()
        .expect("hints")
        .is_empty());
}
#[test]
fn md_output_contains_images_with_order_and_alt_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("with_images.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/gallery",
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
    let images = v["data"]["images"].as_array().expect("images");
    assert_eq!(images.len(), 2, "json images: {v}");
    assert_eq!(
        content.matches("![").count(),
        images.len(),
        "md images must match json images:\n{content}"
    );
    assert!(
        content.contains("![one.webp](https://img.example.com/one.webp)"),
        "empty alt must fall back to filename:\n{content}"
    );
    assert!(content.contains("![second](https://img.example.com/two.png)"));
    let first = content.find("one.webp").expect("first image");
    let para = content.find("Another paragraph").expect("paragraph");
    assert!(
        first < para,
        "image must stay before the following paragraph:\n{content}"
    );
}

#[test]
fn no_images_flag_suppresses_markdown_images_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("with_images.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/gallery",
            "--no-images",
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
    assert!(!content.contains("!["), "no images expected:\n{content}");
    assert_eq!(
        v["data"]["images"].as_array().expect("images").len(),
        2,
        "metadata still lists images"
    );
}

#[test]
fn images_flag_is_accepted_explicitly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("with_images.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/gallery",
            "--images",
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
    assert_eq!(content.matches("![").count(), 2, "{content}");
}

#[test]
fn scope_receipt_reports_hit_and_miss() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");

    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--scope",
            "main",
            "--replay",
            &cassette,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    let scope = &v["meta"]["extract"]["scope"];
    assert_eq!(scope["selector"], "main", "{v}");
    assert_eq!(scope["matched"], true, "{v}");
    assert!(scope["chars_after"].as_u64().is_some(), "{v}");

    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/a",
            "--scope",
            ".does-not-exist",
            "--replay",
            &cassette,
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let v = parse(&out.stdout);
    assert_eq!(v["data"]["content"], "", "miss yields empty content: {v}");
    assert_eq!(
        v["meta"]["extract"]["scope"]["matched"], false,
        "miss must be reported, not silent: {v}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("matched nothing"),
        "humans need a stderr warning, got: {stderr}"
    );
}

#[test]
fn http_429_is_rate_limited_and_retryable_with_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("rate_limited.cassette.json");
    let out = cb(&dir)
        .args(["fetch", "https://example.com/slow", "--replay", &cassette])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["error"]["code"], "rate_limited", "{v}");
    assert_eq!(v["error"]["retryable"], true, "{v}");
    assert_eq!(
        v["error"]["retry_after_s"], 30,
        "Retry-After header must be honoured: {v}"
    );
    assert!(
        v["error"]["needs_user"].is_null(),
        "429 is not a challenge: {v}"
    );
}

#[test]
fn http_429_without_retry_after_defaults_to_five_seconds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("rate_limited_default.cassette.json");
    let out = cb(&dir)
        .args(["fetch", "https://example.com/slow", "--replay", &cassette])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["error"]["code"], "rate_limited", "{v}");
    assert_eq!(v["error"]["retry_after_s"], 5, "{v}");
}

#[test]
fn retryable_failure_switches_profile_and_reports_attempts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("retry_sequence.cassette.json");
    let out = cb(&dir)
        .args(["fetch", "https://example.com/flaky", "--replay", &cassette])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["ok"], true, "{v}");
    let attempts = v["meta"]["attempts"].as_array().expect("meta.attempts");
    assert_eq!(attempts.len(), 2, "one failure + one success: {v}");
    assert_eq!(attempts[0]["profile"], "chrome", "{v}");
    assert_eq!(attempts[0]["ok"], false, "{v}");
    assert_eq!(attempts[0]["error"], "timeout", "{v}");
    assert_eq!(attempts[1]["profile"], "firefox", "{v}");
    assert_eq!(attempts[1]["ok"], true, "{v}");
    assert!(
        attempts[1].get("error").is_none_or(|e| e.is_null()),
        "successful attempt has no error: {v}"
    );
}

#[test]
fn all_profiles_failing_reports_three_attempts_and_total_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("retry_all_fail.cassette.json");
    let out = cb(&dir)
        .args(["fetch", "https://example.com/flaky", "--replay", &cassette])
        .assert()
        .code(7)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "timeout", "{v}");
    let attempts = v["meta"]["attempts"].as_array().expect("meta.attempts");
    assert_eq!(attempts.len(), 3, "must try all three profiles: {v}");
    let profiles: Vec<&str> = attempts
        .iter()
        .map(|a| a["profile"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(profiles, vec!["chrome", "firefox", "safari"], "{v}");
    let message = v["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("all 3 profiles failed"),
        "message must aggregate attempts: {message}"
    );
    assert!(message.contains("chrome:timeout"), "{message}");
}

#[test]
fn not_found_is_not_retried() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("not_found.cassette.json");
    let out = cb(&dir)
        .args([
            "fetch",
            "https://example.com/missing",
            "--replay",
            &cassette,
        ])
        .assert()
        .code(6)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["error"]["code"], "not_found", "{v}");
    let attempts = v["meta"]["attempts"].as_array().expect("meta.attempts");
    assert_eq!(attempts.len(), 1, "404 must not be retried: {v}");
}

#[test]
#[ignore = "needs a network stack (uses a blackhole IP to prove connect_timeout)"]
fn connect_timeout_fails_fast_on_blackhole_ip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let started = std::time::Instant::now();
    let out = cb(&dir)
        .args(["fetch", "https://10.255.255.1/", "--fresh"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let elapsed = started.elapsed();
    let v = parse(&out);
    let attempts = v["meta"]["attempts"].as_array().expect("meta.attempts");
    assert!(!attempts.is_empty());
    for a in attempts {
        let ms = a["ms"].as_u64().unwrap_or(u64::MAX);
        assert!(
            ms < 8_000,
            "connect timeout 5s must cut each attempt short, got {ms}ms: {v}"
        );
    }
    assert!(
        elapsed < std::time::Duration::from_secs(40),
        "whole ladder must not run for minutes: {elapsed:?}"
    );
}

#[test]
#[ignore = "needs a real DNS lookup (offline CI cannot resolve anything)"]
fn missing_domain_reports_dns_error_not_network() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = cb(&dir)
        .args(["fetch", "https://this-domain-does-not-exist-xyz.example"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v = parse(&out);
    assert_eq!(v["error"]["code"], "dns_error", "{v}");
    assert_eq!(v["error"]["retryable"], false, "{v}");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("this-domain-does-not-exist-xyz"),
        "message must name the host: {v}"
    );
}
