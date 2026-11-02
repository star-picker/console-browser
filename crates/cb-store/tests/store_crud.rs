use cb_store::{CacheEntry, HistoryRow, LogRow, PageRow, Store};

fn tmp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn sample_page(url: &str) -> PageRow {
    PageRow {
        id: 0,
        url: url.to_string(),
        url_hash: cb_store::hash_url(url),
        canonical: url.to_string(),
        site: "example.com".to_string(),
        title: "Example".to_string(),
        status: 200,
        content_type: "text/html".to_string(),
        fetched_at: 1_700_000_000,
        content_sha: "abc123".to_string(),
        md: b"# Hello\n\nworld".to_vec(),
        raw: None,
        etag: Some("\"v1\"".to_string()),
        last_modified: None,
        bytes: 42,
        tokens: 7,
        lang: Some("en".to_string()),
    }
}

#[test]
fn schema_created_and_crud_roundtrip() {
    let dir = tmp_dir();
    let path = dir.path().join("cb.db");
    let store = Store::open(&path).expect("open");
    assert!(store.schema_version().expect("version") >= 1);

    let id = store
        .put_page(&sample_page("https://example.com/a"))
        .expect("put");
    assert!(id > 0);

    let got = store
        .get_page_by_url("https://example.com/a")
        .expect("get")
        .expect("some");
    assert_eq!(got.title, "Example");
    assert_eq!(got.md, b"# Hello\n\nworld");
    assert_eq!(got.etag.as_deref(), Some("\"v1\""));
    assert!(store
        .get_page_by_url("https://nope.example/")
        .expect("get2")
        .is_none());

    // reopening the same file keeps data and migrations are idempotent
    drop(store);
    let store = Store::open(&path).expect("reopen");
    assert_eq!(
        store.schema_version().expect("version"),
        cb_store::SCHEMA_VERSION
    );
    assert!(store
        .get_page_by_url("https://example.com/a")
        .expect("get3")
        .is_some());
}

#[test]
fn cache_put_get_and_expiry() {
    let store = Store::open_in_memory().expect("open");
    let entry = CacheEntry {
        key: "k1".to_string(),
        method: "GET".to_string(),
        url: "https://example.com/".to_string(),
        status: 200,
        headers: "{\"content-type\":\"text/html\"}".to_string(),
        body: b"<html></html>".to_vec(),
        expires_at: 2_000_000_000,
        validators: "{\"etag\":\"v1\"}".to_string(),
        created_at: 1_700_000_000,
    };
    store.cache_put(&entry).expect("put");
    let got = store.cache_get("k1").expect("get").expect("some");
    assert_eq!(got.body, b"<html></html>");
    assert_eq!(got.status, 200);
    assert!(store.cache_get("missing").expect("get2").is_none());
}

#[test]
fn history_is_append_only_and_queryable() {
    let store = Store::open_in_memory().expect("open");
    for (i, url) in ["https://example.com/a", "https://example.com/b"]
        .iter()
        .enumerate()
    {
        store
            .append_history(&HistoryRow {
                id: 0,
                ts: 1_700_000_000 + i as i64,
                url: url.to_string(),
                title: format!("t{i}"),
                kind: "fetch".to_string(),
                session_id: None,
                duration_ms: 10,
                ok: true,
            })
            .expect("append");
    }
    let all = store.list_history(None, 10).expect("list");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].url, "https://example.com/b", "newest first");
    let filtered = store
        .list_history(Some("example.com/a"), 10)
        .expect("list2");
    assert_eq!(filtered.len(), 1);
}

#[test]
fn logs_append_and_prune_reports_counts() {
    let store = Store::open_in_memory().expect("open");
    store
        .append_log(&LogRow {
            id: 0,
            ts: 100,
            level: "INFO".to_string(),
            target: "cb.test".to_string(),
            message: "hello".to_string(),
            fields: None,
        })
        .expect("log");
    let report = store.prune(1_000).expect("prune");
    assert_eq!(report.logs_deleted, 0);
    assert_eq!(report.history_deleted, 0);
}
