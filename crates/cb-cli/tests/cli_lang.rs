use assert_cmd::Command;

fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3000}'..='\u{303f}').contains(&c))
}

fn run(args: &[&str], stdin: Option<&str>) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("cb").expect("bin");
    cmd.arg("--db")
        .arg(dir.path().join("cb.db"))
        .env_remove("CB_LANG")
        .args(args);
    let out = cmd.output().expect("run");
    let _ = stdin;
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn english_help_has_no_chinese() {
    let text = run(&["--lang", "en", "--help"], None);
    assert!(text.contains("console-browser"), "{text}");
    assert!(!has_cjk(&text), "english help leaked chinese:\n{text}");
}

#[test]
fn chinese_help_has_chinese() {
    let text = run(&["--lang", "zh", "--help"], None);
    assert!(has_cjk(&text), "chinese help missing chinese:\n{text}");
}

#[test]
fn per_command_help_follows_language() {
    let en = run(&["--lang", "en", "fetch", "--help"], None);
    assert!(!has_cjk(&en), "fetch help (en) leaked chinese:\n{en}");
    assert!(en.contains("--max-tokens"), "{en}");

    let zh = run(&["--lang", "zh", "fetch", "--help"], None);
    assert!(has_cjk(&zh), "fetch help (zh) missing chinese:\n{zh}");
}

#[test]
fn tools_listing_is_single_language() {
    let en = run(&["--lang", "en", "tools"], None);
    assert!(en.contains("Fetch a page"), "{en}");
    assert!(!has_cjk(&en), "tools (en) leaked chinese:\n{en}");

    let zh = run(&["--lang", "zh", "tools"], None);
    assert!(has_cjk(&zh), "tools (zh) missing chinese:\n{zh}");
}
