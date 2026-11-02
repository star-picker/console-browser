use cb_net::{cache_key, NetClient, Profile, Request};

const CASSETTE: &str = r#"{"version":1,"entries":[
  {"url":"https://example.com/a","final_url":"https://example.com/a","status":200,
   "headers":{"content-type":"text/html; charset=utf-8","etag":"\"v1\""},
   "body":"<html><body><h1>Hi</h1></body></html>"}
]}"#;

fn write_cassette() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("case.cassette.json");
    std::fs::write(&path, CASSETTE).expect("write cassette");
    (dir, path)
}

#[tokio::test]
async fn replay_returns_recorded_status_headers_body() {
    let (_dir, path) = write_cassette();
    let client = NetClient::replay(&path).expect("replay client");

    let resp = client
        .get(&Request::get("https://example.com/a"))
        .await
        .expect("get");
    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(resp.header("etag"), Some("\"v1\""));
    assert_eq!(resp.final_url, "https://example.com/a");
    assert!(resp.text().contains("<h1>Hi</h1>"));

    let err = client
        .get(&Request::get("https://example.com/missing"))
        .await
        .expect_err("unrecorded url must fail");
    assert!(format!("{err}").contains("not recorded"), "got: {err}");
}

#[test]
fn cache_key_is_stable_and_discriminating() {
    let a = cache_key("GET", "https://example.com/a", Profile::Chrome, false);
    let b = cache_key("GET", "https://example.com/a", Profile::Chrome, false);
    assert_eq!(a, b, "same input -> same key");

    assert_eq!(
        cache_key("GET", "https://EXAMPLE.com/a#frag", Profile::Chrome, false),
        cache_key("GET", "https://example.com/a", Profile::Chrome, false),
        "host case and fragment are normalized away"
    );

    assert_ne!(
        a,
        cache_key("GET", "https://example.com/a", Profile::Firefox, false)
    );
    assert_ne!(
        a,
        cache_key("GET", "https://example.com/a", Profile::Chrome, true)
    );
    assert_ne!(
        a,
        cache_key("POST", "https://example.com/a", Profile::Chrome, false)
    );
}
