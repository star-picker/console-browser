//! MCP stdio 握手：进程内对打，不需要网络。

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn run_session(
    dir: &tempfile::TempDir,
    cassette: &str,
    requests: &[&str],
) -> Vec<serde_json::Value> {
    let exe = assert_cmd::cargo::cargo_bin("cb");
    let mut child = Command::new(exe)
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("mcp")
        .arg("serve")
        .arg("--replay")
        .arg(cassette)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cb mcp serve");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            writeln!(stdin, "{req}").expect("write request");
        }
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());

    let out = BufReader::new(child.stdout.take().expect("stdout"));
    let mut responses = Vec::new();
    for line in out.lines() {
        let line = line.expect("line");
        if line.trim().is_empty() {
            continue;
        }
        responses.push(serde_json::from_str(&line).unwrap_or_else(|e| {
            panic!("response is not json ({e}): {line}");
        }));
    }
    let status = child.wait().expect("wait");
    assert!(status.success(), "server exit: {status:?}");
    responses
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;

#[test]
fn initialize_list_and_call_browse_url() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"browse_url","arguments":{"url":"https://example.com/a"}}}"#;
    let responses = run_session(&dir, &cassette, &[INIT, INITIALIZED, LIST, call]);

    assert_eq!(
        responses.len(),
        3,
        "one response per request with id: {responses:?}"
    );

    let init = &responses[0];
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "cb");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    let list = &responses[1];
    let tools = list["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"browse_url"), "got {names:?}");
    for t in tools {
        assert!(t["inputSchema"].is_object(), "MCP needs inputSchema: {t}");
    }

    let call = &responses[2];
    assert_eq!(call["id"], 3);
    assert_eq!(call["result"]["isError"], false);
    let text = call["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("# Hello World"), "got: {text}");
}

#[test]
fn unknown_tool_reports_error_without_killing_server() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let call =
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#;
    let responses = run_session(&dir, &cassette, &[INIT, LIST, call]);
    assert_eq!(responses.len(), 3);
    let call = &responses[2];
    assert_eq!(call["result"]["isError"], true);
}
#[test]
fn invalid_utf8_line_does_not_kill_the_server() {
    use std::io::Write as _;

    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let exe = assert_cmd::cargo::cargo_bin("cb");
    let mut child = Command::new(exe)
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .arg("mcp")
        .arg("serve")
        .arg("--replay")
        .arg(cassette)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        // 非 UTF-8 字节行，然后是正常的 initialize
        stdin.write_all(b"\xff\xfe not utf8\n").expect("write junk");
        writeln!(stdin, "{INIT}").expect("write init");
        stdin.flush().expect("flush");
    }
    drop(child.stdin.take());

    let out = BufReader::new(child.stdout.take().expect("stdout"));
    let mut responses = Vec::new();
    for line in out.lines() {
        let line = line.expect("line");
        if line.trim().is_empty() {
            continue;
        }
        responses.push(serde_json::from_str::<serde_json::Value>(&line).expect("json"));
    }
    let status = child.wait().expect("wait");
    assert!(
        status.success(),
        "server must exit clean on EOF, got {status:?}"
    );
    assert!(
        responses
            .iter()
            .any(|r| r["id"] == 1 && r["result"]["serverInfo"]["name"] == "cb"),
        "initialize must still be answered after a bad line: {responses:?}"
    );
}
