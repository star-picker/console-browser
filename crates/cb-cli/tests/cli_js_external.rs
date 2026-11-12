use assert_cmd::Command;

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn run(dir: &tempfile::TempDir, extra: &[&str]) -> (i32, String) {
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .args([
            "js",
            "https://site.example/page",
            "--run-inline",
            "--replay",
            &fixture("js_external.cassette.json"),
        ])
        .args(extra);
    let out = cmd.assert().get_output().clone();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

#[test]
fn external_scripts_are_skipped_by_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run(&dir, &[]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("Plain Title"),
        "must keep page title: {stdout}"
    );
}

#[test]
fn allowlisted_external_script_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run(&dir, &["--external-allow", "cdn.example"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("External OK"),
        "external script must run: {stdout}"
    );
}

#[test]
fn non_allowlisted_host_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run(&dir, &["--external-allow", "other.example"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        !stdout.contains("External OK"),
        "non-allowlisted host ran: {stdout}"
    );
}
