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
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CB_AI_KEY")
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
const INIT_SAMPLING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{"sampling":{}},"clientInfo":{"name":"test","version":"0"}}}"#;
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

#[test]
fn search_web_tool_is_exposed_and_callable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("gh.cassette.json");
    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_web","arguments":{"query":"ripgrep","engine":"gh","limit":5}}}"#;
    let responses = run_session(&dir, &cassette, &[INIT, INITIALIZED, list, call]);
    assert_eq!(
        responses.len(),
        3,
        "responses: {}",
        serde_json::to_string(&responses).unwrap_or_default()
    );
    let tools = responses[1]["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"search_web"), "got {names:?}");
    assert_eq!(responses[2]["result"]["isError"], false);
    let text = responses[2]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("BurntSushi/ripgrep"), "got: {text}");
}

#[test]
fn ask_model_without_sampling_capability_reports_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cassette = fixture("page.cassette.json");
    let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ask_model","arguments":{"prompt":"ping"}}}"#;
    let responses = run_session(&dir, &cassette, &[INIT, LIST, call]);
    assert_eq!(responses.len(), 3);
    let call = &responses[2];
    assert_eq!(call["result"]["isError"], true, "{call}");
    let text = call["result"]["content"][0]["text"].as_str().expect("text");
    assert!(
        text.contains("sampling"),
        "error must explain sampling is unavailable: {text}"
    );
}

#[test]
fn sampling_request_is_sent_and_answered() {
    use std::io::{BufRead, BufReader, Write};
    let dir = tempfile::tempdir().expect("tempdir");
    let exe = assert_cmd::cargo::cargo_bin("cb");
    let mut child = Command::new(exe)
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .args(["mcp", "serve"])
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CB_AI_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cb mcp serve");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    let read = |reader: &mut BufReader<std::process::ChildStdout>| -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not json ({e}): {line}"))
    };

    writeln!(stdin, "{INIT_SAMPLING}").expect("write init");
    stdin.flush().expect("flush");
    let init = read(&mut stdout);
    assert_eq!(init["id"], 1);

    let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ask_model","arguments":{"prompt":"ping"}}}"#;
    writeln!(stdin, "{call}").expect("write call");
    stdin.flush().expect("flush");

    let sampling = read(&mut stdout);
    assert_eq!(sampling["method"], "sampling/createMessage", "{sampling}");
    let text = sampling["params"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        text.contains("ping"),
        "prompt must reach the client: {sampling}"
    );
    let sampling_id = sampling["id"].as_str().expect("string id").to_string();

    let answer = serde_json::json!({
        "jsonrpc": "2.0",
        "id": sampling_id,
        "result": {
            "role": "assistant",
            "content": { "type": "text", "text": "sampled-pong" },
            "model": "client-model",
        }
    });
    writeln!(stdin, "{answer}").expect("write sampling answer");
    stdin.flush().expect("flush");

    let tool = read(&mut stdout);
    assert_eq!(tool["id"], 3, "{tool}");
    assert_eq!(tool["result"]["isError"], false, "{tool}");
    let text = tool["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(text, "sampled-pong", "{tool}");

    drop(stdin);
    let status = child.wait().expect("wait");
    assert!(status.success(), "server exit: {status:?}");
}

#[test]
fn challenge_escalation_uses_client_sampling_when_no_local_key() {
    use std::io::{BufRead, BufReader, Write};
    let dir = tempfile::tempdir().expect("tempdir");
    let exe = assert_cmd::cargo::cargo_bin("cb");
    let mut child = Command::new(exe)
        .arg("--db")
        .arg(dir.path().join("cb.db"))
        .args(["mcp", "serve"])
        .arg("--replay")
        .arg(fixture("challenge_403.cassette.json"))
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CB_AI_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cb mcp serve");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let read = |reader: &mut BufReader<std::process::ChildStdout>| -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not json ({e}): {line}"))
    };

    writeln!(stdin, "{INIT_SAMPLING}").expect("write init");
    stdin.flush().expect("flush");
    let _ = read(&mut stdout);

    let call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"browse_url","arguments":{"url":"https://blocked.example/challenge","ai_escalate":true}}}"#;
    writeln!(stdin, "{call}").expect("write call");
    stdin.flush().expect("flush");

    let sampling = read(&mut stdout);
    assert_eq!(sampling["method"], "sampling/createMessage", "{sampling}");
    let prompt = sampling["params"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        prompt.contains("anti-bot") || prompt.contains("blocked"),
        "challenge prompt must carry the situation: {sampling}"
    );
    let id = sampling["id"].as_str().expect("string id").to_string();
    let answer = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "role": "assistant",
            "content": { "type": "text", "text": "sampled challenge advice" },
            "model": "client-model",
        }
    });
    writeln!(stdin, "{answer}").expect("write answer");
    stdin.flush().expect("flush");

    let tool = read(&mut stdout);
    assert_eq!(tool["result"]["isError"], true, "{tool}");
    let text = tool["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        text.contains("sampled challenge advice"),
        "advice must reach the caller through the hint: {tool}"
    );

    drop(stdin);
    let status = child.wait().expect("wait");
    assert!(status.success(), "server exit: {status:?}");
}
