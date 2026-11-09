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

fn plugin_dir(dir: &tempfile::TempDir, perms: &str, hosts: &str, selector: &str) -> String {
    let root = dir.path().join("plugins").join("scoped");
    std::fs::create_dir_all(&root).expect("mkdir");
    let manifest = format!(
        r#"{{"name":"scoped","version":"1.0.0","permissions":[{perms}],"match":[{hosts}],"hooks":{{"extract":{{"selectors":["{selector}"]}}}}}}"#
    );
    std::fs::write(root.join("plugin.json"), manifest).expect("write");
    dir.path().join("plugins").to_string_lossy().to_string()
}

fn fetch_md(dir: &tempfile::TempDir, plugins: &str) -> (i32, Value) {
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("--json")
        .env("CB_PLUGIN_DIR", plugins)
        .args([
            "fetch",
            "https://example.com/a",
            "--replay",
            &fixture("page.cassette.json"),
        ]);
    let out = cmd.assert().get_output().clone();
    let code = out.status.code().unwrap_or(-1);
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not json ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, v)
}

#[test]
fn plugin_selector_scopes_extraction_for_matching_host() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plugins = plugin_dir(&dir, "\"extract\"", "\"example.com\"", "h1");
    let (code, v) = fetch_md(&dir, &plugins);
    assert_eq!(code, 0, "{v}");
    let md = v["data"]["content"].as_str().expect("content");
    assert!(md.contains("Hello World"), "{md}");
    assert!(
        !md.contains("paragraph exists"),
        "scope must cut the rest of the page: {md}"
    );
}

#[test]
fn plugin_without_extract_permission_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plugins = plugin_dir(&dir, "", "\"example.com\"", "h1");
    let (code, v) = fetch_md(&dir, &plugins);
    assert_eq!(code, 0, "{v}");
    let md = v["data"]["content"].as_str().expect("content");
    assert!(
        md.contains("paragraph exists"),
        "no permission = no scoping: {md}"
    );
}
