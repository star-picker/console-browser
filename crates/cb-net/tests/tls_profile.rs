//! 手工校验：本机指纹是否与所选 browser preset 一致。
//! 需要联网，默认 `#[ignore]`；CI 不跑。
//! 运行：`cargo test -p cb-net --test tls_profile -- --ignored --nocapture`

use cb_net::{NetClient, Profile, Request};

async fn oracle_json(url: &str) -> serde_json::Value {
    let client = NetClient::new(Profile::Chrome).expect("live client");
    let resp = client.get(&Request::get(url)).await.expect("oracle get");
    serde_json::from_str(&resp.text()).unwrap_or(serde_json::json!({}))
}

#[tokio::test]
#[ignore = "requires network: validates TLS fingerprint against an oracle"]
async fn fingerprint_looks_like_chrome() {
    let v = oracle_json("https://tls.peet.ws/api/all").await;
    let ja3 = v["tls"]["ja3_hash"].as_str().unwrap_or_default();
    let ua = v["user_agent"].as_str().unwrap_or_default();
    println!("ja3={ja3} ua={ua}");
    assert!(!ja3.is_empty(), "oracle did not report ja3");
    assert!(
        ua.contains("Chrome"),
        "user agent should look like Chrome, got {ua}"
    );
}
