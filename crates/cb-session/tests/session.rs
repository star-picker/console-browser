//! 会话表 + 差量 + daemon 协议的离线测试。

use cb_extract::{Block, Span};
use cb_session::{diff_blocks, BlockOp, Daemon, Request, SnapshotKind, SnapshotView};
use serde_json::{json, Value};

fn para(text: &str) -> Block {
    Block::Paragraph {
        spans: vec![Span::normal(text)],
    }
}

fn block_texts(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { spans } => Some(
                spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn diff_replaces_changed_block_in_place() {
    let old = vec![para("a"), para("b"), para("c")];
    let new = vec![para("a"), para("x"), para("c")];
    let ops = diff_blocks(&old, &new);
    assert_eq!(ops.len(), 1, "{ops:?}");
    match &ops[0] {
        BlockOp::Replace { index, block } => {
            assert_eq!(*index, 1);
            assert_eq!(block_texts(std::slice::from_ref(block)), vec!["x"]);
        }
        other => panic!("expected replace, got {other:?}"),
    }
}

#[test]
fn diff_inserts_and_removes_at_block_granularity() {
    let old = vec![para("a"), para("c")];
    let new = vec![para("a"), para("b"), para("c")];
    assert_eq!(diff_blocks(&old, &new).len(), 1);
    assert!(matches!(
        diff_blocks(&old, &new)[0],
        BlockOp::Insert { index: 1, .. }
    ));

    let removed = diff_blocks(&new, &old);
    assert_eq!(removed.len(), 1);
    assert!(matches!(removed[0], BlockOp::Remove { index: 1 }));

    assert!(diff_blocks(&old, &old).is_empty());
}

#[test]
fn diff_handles_length_changes_mid_document() {
    let old = vec![para("a"), para("b"), para("c")];
    let new = vec![para("a"), para("x"), para("y"), para("c")];
    let ops = diff_blocks(&old, &new);
    assert!(
        ops.iter()
            .any(|op| matches!(op, BlockOp::Remove { index: 1 })),
        "{ops:?}"
    );
    let inserts: Vec<usize> = ops
        .iter()
        .filter_map(|op| match op {
            BlockOp::Insert { index, .. } => Some(*index),
            _ => None,
        })
        .collect();
    assert_eq!(inserts, vec![1, 2], "{ops:?}");
}

fn cassette(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let body = "<html><head><title>Session Page</title></head><body><main><h1>Hello</h1><p>This paragraph is long enough for the main content scorer to pick the container and produce blocks.</p></main></body></html>";
    let payload = json!({
        "version": 1,
        "entries": [{
            "url": "https://example.com/session",
            "final_url": "https://example.com/session",
            "status": 200,
            "headers": {"content-type": "text/html; charset=utf-8"},
            "body": body,
        }]
    });
    let path = dir.path().join("session.cassette.json");
    std::fs::write(&path, serde_json::to_string_pretty(&payload).expect("json")).expect("write");
    path
}

fn daemon(replay: &std::path::Path) -> Daemon {
    let store = cb_store::Store::open_in_memory().expect("store");
    let ctx = cb_core::Ctx::new(store, cb_net::Profile::Chrome);
    Daemon::new(ctx, Some(replay.to_path_buf()))
}

async fn call(daemon: &mut Daemon, method: &str, params: Value) -> (bool, Value) {
    let response = daemon
        .call(Request {
            id: 7,
            method: method.to_string(),
            params,
        })
        .await;
    assert_eq!(response.id, 7);
    (response.ok, response.result.unwrap_or(Value::Null))
}

async fn open(daemon: &mut Daemon) -> SnapshotView {
    let (ok, result) = call(
        daemon,
        "open",
        json!({ "url": "https://example.com/session" }),
    )
    .await;
    assert!(ok, "{result}");
    serde_json::from_value(result).expect("snapshot view")
}

#[tokio::test]
async fn open_assigns_handle_and_get_supports_unchanged_and_delta() {
    let dir = tempfile::tempdir().expect("tempdir");
    let replay = cassette(&dir);
    let mut daemon = daemon(&replay);

    let opened = open(&mut daemon).await;
    assert_eq!(opened.handle, "@1");
    assert_eq!(opened.revision, 1);
    assert_eq!(opened.kind, SnapshotKind::Full);
    assert_eq!(opened.title, "Session Page");
    let blocks = opened.blocks.expect("full snapshot carries blocks");
    assert!(
        blocks
            .iter()
            .any(|block| block_texts(std::slice::from_ref(block))
                .iter()
                .any(|text| text.contains("long enough"))),
        "{blocks:?}"
    );

    let (ok, result) = call(&mut daemon, "get", json!({ "handle": "@1", "since": 1 })).await;
    assert!(ok);
    let unchanged: SnapshotView = serde_json::from_value(result).expect("view");
    assert_eq!(unchanged.kind, SnapshotKind::Unchanged);
    assert!(unchanged.blocks.is_none());
    assert!(unchanged.ops.is_empty());

    let (ok, result) = call(&mut daemon, "refresh", json!({ "handle": "@1" })).await;
    assert!(ok, "{result}");
    let refreshed: SnapshotView = serde_json::from_value(result).expect("view");
    assert_eq!(refreshed.revision, 2);
    assert_eq!(refreshed.kind, SnapshotKind::Full);

    let (ok, result) = call(&mut daemon, "get", json!({ "handle": "@1", "since": 1 })).await;
    assert!(ok);
    let delta: SnapshotView = serde_json::from_value(result).expect("view");
    assert_eq!(delta.kind, SnapshotKind::Delta);
    assert_eq!(delta.revision, 2);
    assert!(delta.blocks.is_none());
    assert!(
        delta.ops.is_empty(),
        "same content means no ops: {:?}",
        delta.ops
    );

    let (ok, result) = call(&mut daemon, "get", json!({ "handle": "@1", "since": 0 })).await;
    assert!(ok);
    let stale: SnapshotView = serde_json::from_value(result).expect("view");
    assert_eq!(stale.kind, SnapshotKind::Full, "unknown revision -> full");
}

#[tokio::test]
async fn list_and_close_track_handles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let replay = cassette(&dir);
    let mut daemon = daemon(&replay);
    let opened = open(&mut daemon).await;
    assert_eq!(opened.handle, "@1");

    let (ok, result) = call(&mut daemon, "list", json!({})).await;
    assert!(ok);
    let sessions = result["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["handle"], "@1");

    let (ok, result) = call(&mut daemon, "close", json!({ "handle": "@1" })).await;
    assert!(ok, "{result}");
    assert_eq!(result["closed"], "@1");
    assert_eq!(daemon.handle_count(), 0);

    let (ok, result) = call(&mut daemon, "get", json!({ "handle": "@1" })).await;
    assert!(!ok, "{result}");
}

#[tokio::test]
async fn unknown_method_and_missing_params_are_usage_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let replay = cassette(&dir);
    let mut daemon = daemon(&replay);

    let response = daemon.call(Request::new("nope", json!({}))).await;
    assert!(!response.ok);
    assert_eq!(response.error.expect("error").code, "usage");

    let response = daemon.call(Request::new("open", json!({}))).await;
    assert!(!response.ok);
    assert_eq!(response.error.expect("error").code, "usage");
}

#[tokio::test]
async fn ping_reports_engine_and_session_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    let replay = cassette(&dir);
    let mut daemon = daemon(&replay);
    let (ok, result) = call(&mut daemon, "ping", json!({})).await;
    assert!(ok);
    assert_eq!(result["pong"], true);
    assert!(result["version"]
        .as_str()
        .unwrap_or_default()
        .starts_with("cb/"));
}
