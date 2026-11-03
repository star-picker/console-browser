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
#[test]
fn tabs_session_roundtrip() {
    let store = Store::open_in_memory().expect("open");
    store
        .save_tabs(&[
            cb_store::TabRow {
                slot: 0,
                url: "https://a.example/".to_string(),
                title: "A".to_string(),
                scroll: 3,
                nav_json: "[\"https://a.example/\"]".to_string(),
            },
            cb_store::TabRow {
                slot: 1,
                url: "https://b.example/".to_string(),
                title: "B".to_string(),
                scroll: 0,
                nav_json: "[\"https://b.example/\",\"https://b.example/2\"]".to_string(),
            },
        ])
        .expect("save");
    let rows = store.load_tabs().expect("load");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].url, "https://b.example/");
    assert_eq!(rows[1].scroll, 0);

    // 快照语义：再存一次更少的标签，旧的必须消失
    store.save_tabs(&[]).expect("clear");
    assert!(store.load_tabs().expect("load").is_empty());
}
#[test]
fn history_fts_matches_title_and_url() {
    let store = cb_store::Store::open_in_memory().expect("store");
    for (title, url) in [
        ("rust async runtime", "https://a.example/rust"),
        ("python packaging", "https://b.example/python"),
    ] {
        store
            .append_history(&cb_store::HistoryRow {
                id: 0,
                ts: 1_700_000_000,
                url: url.to_string(),
                title: title.to_string(),
                kind: "fetch".to_string(),
                session_id: None,
                duration_ms: 1,
                ok: true,
            })
            .expect("append");
    }
    let hits = store.search_history_fts("rust", 10).expect("fts");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].title.contains("rust"));
    let url_hits = store.search_history_fts("python", 10).expect("fts");
    assert_eq!(url_hits.len(), 1);
    assert!(store
        .search_history_fts("nonexistentterm", 10)
        .expect("fts")
        .is_empty());
}
#[test]
fn site_strategy_counts_success_and_failure() {
    let store = cb_store::Store::open_in_memory().expect("store");
    store
        .record_site("example.com", "chrome", true)
        .expect("ok1");
    store
        .record_site("example.com", "firefox", true)
        .expect("ok2");
    store
        .record_site("blocked.example", "chrome", false)
        .expect("fail");
    let sites = store.list_sites().expect("list");
    let example = sites
        .iter()
        .find(|s| s.domain == "example.com")
        .expect("example");
    assert_eq!(example.profile, "firefox", "last successful profile wins");
    assert_eq!(example.ok_count, 2, "ok count");
    assert_eq!(example.fail_count, 0, "fail count");
    let blocked = sites
        .iter()
        .find(|s| s.domain == "blocked.example")
        .expect("blocked");
    assert_eq!(blocked.fail_count, 1);
}
#[test]
fn revisions_dedupe_identical_content() {
    let store = cb_store::Store::open_in_memory().expect("store");
    assert!(store
        .save_revision("https://a/", "sha1", 100, 2)
        .expect("r1"));
    assert!(!store
        .save_revision("https://a/", "sha1", 100, 2)
        .expect("same sha must not add a row"));
    assert!(store
        .save_revision("https://a/", "sha2", 140, 3)
        .expect("r2"));
    let rows = store.list_revisions("https://a/").expect("list");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].sha, "sha2", "newest first");
    assert_eq!(rows[0].chars, 140);
    assert_eq!(rows[1].sha, "sha1");
}
