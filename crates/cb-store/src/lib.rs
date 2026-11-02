//! cb-store: local sqlite storage (pages, cache, history, logs, settings).

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const SCHEMA_VERSION: i32 = 1;

/// 缓存保留 7 天，历史 90 天，日志 14 天。
pub const CACHE_RETENTION_SECS: i64 = 7 * 86_400;
pub const HISTORY_RETENTION_SECS: i64 = 90 * 86_400;
pub const LOG_RETENTION_SECS: i64 = 14 * 86_400;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS pages(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  url TEXT NOT NULL UNIQUE,
  url_hash TEXT NOT NULL,
  canonical TEXT NOT NULL,
  site TEXT NOT NULL,
  title TEXT NOT NULL,
  status INTEGER NOT NULL,
  content_type TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  content_sha TEXT NOT NULL,
  md BLOB NOT NULL,
  raw BLOB,
  etag TEXT,
  last_modified TEXT,
  bytes INTEGER NOT NULL,
  tokens INTEGER NOT NULL,
  lang TEXT
);
CREATE INDEX IF NOT EXISTS idx_pages_content_sha ON pages(content_sha);
CREATE TABLE IF NOT EXISTS cache_entries(
  key TEXT PRIMARY KEY,
  method TEXT NOT NULL,
  url TEXT NOT NULL,
  status INTEGER NOT NULL,
  headers TEXT NOT NULL,
  body BLOB NOT NULL,
  expires_at INTEGER NOT NULL,
  validators TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cache_expires ON cache_entries(expires_at);
CREATE TABLE IF NOT EXISTS history(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  kind TEXT NOT NULL,
  session_id TEXT,
  duration_ms INTEGER NOT NULL,
  ok INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_history_ts ON history(ts DESC);
CREATE TABLE IF NOT EXISTS logs(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL,
  level TEXT NOT NULL,
  target TEXT NOT NULL,
  message TEXT NOT NULL,
  fields TEXT
);
CREATE TABLE IF NOT EXISTS settings(
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRow {
    pub id: i64,
    pub url: String,
    pub url_hash: String,
    pub canonical: String,
    pub site: String,
    pub title: String,
    pub status: i64,
    pub content_type: String,
    pub fetched_at: i64,
    pub content_sha: String,
    pub md: Vec<u8>,
    pub raw: Option<Vec<u8>>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub bytes: i64,
    pub tokens: i64,
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub method: String,
    pub url: String,
    pub status: i64,
    pub headers: String,
    pub body: Vec<u8>,
    pub expires_at: i64,
    pub validators: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRow {
    pub id: i64,
    pub ts: i64,
    pub url: String,
    pub title: String,
    pub kind: String,
    pub session_id: Option<String>,
    pub duration_ms: i64,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRow {
    pub id: i64,
    pub ts: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneReport {
    pub cache_deleted: i64,
    pub history_deleted: i64,
    pub logs_deleted: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    pub pages: i64,
    pub cache_entries: i64,
    pub history: i64,
    pub logs: i64,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn prepare(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < SCHEMA_VERSION {
        conn.execute_batch(SCHEMA_SQL).context("apply schema")?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create db dir {:?}", parent))?;
            }
        }
        let conn = Connection::open(path).with_context(|| format!("open db {:?}", path))?;
        prepare(&conn)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Store> {
        let conn = Connection::open_in_memory()?;
        prepare(&conn)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn schema_version(&self) -> Result<i32> {
        let conn = self.lock();
        Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn put_page(&self, page: &PageRow) -> Result<i64> {
        let conn = self.lock();
        let id: i64 = conn.query_row(
            r#"INSERT INTO pages (url,url_hash,canonical,site,title,status,content_type,fetched_at,
                                 content_sha,md,raw,etag,last_modified,bytes,tokens,lang)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
               ON CONFLICT(url) DO UPDATE SET
                 url_hash=excluded.url_hash, canonical=excluded.canonical, site=excluded.site,
                 title=excluded.title, status=excluded.status, content_type=excluded.content_type,
                 fetched_at=excluded.fetched_at, content_sha=excluded.content_sha, md=excluded.md,
                 raw=excluded.raw, etag=excluded.etag, last_modified=excluded.last_modified,
                 bytes=excluded.bytes, tokens=excluded.tokens, lang=excluded.lang
               RETURNING id"#,
            params![
                page.url,
                page.url_hash,
                page.canonical,
                page.site,
                page.title,
                page.status,
                page.content_type,
                page.fetched_at,
                page.content_sha,
                page.md,
                page.raw,
                page.etag,
                page.last_modified,
                page.bytes,
                page.tokens,
                page.lang
            ],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn get_page_by_url(&self, url: &str) -> Result<Option<PageRow>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                r#"SELECT id,url,url_hash,canonical,site,title,status,content_type,fetched_at,
                          content_sha,md,raw,etag,last_modified,bytes,tokens,lang
                   FROM pages WHERE url=?1"#,
                params![url],
                |r| {
                    Ok(PageRow {
                        id: r.get(0)?,
                        url: r.get(1)?,
                        url_hash: r.get(2)?,
                        canonical: r.get(3)?,
                        site: r.get(4)?,
                        title: r.get(5)?,
                        status: r.get(6)?,
                        content_type: r.get(7)?,
                        fetched_at: r.get(8)?,
                        content_sha: r.get(9)?,
                        md: r.get(10)?,
                        raw: r.get(11)?,
                        etag: r.get(12)?,
                        last_modified: r.get(13)?,
                        bytes: r.get(14)?,
                        tokens: r.get(15)?,
                        lang: r.get(16)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn cache_put(&self, entry: &CacheEntry) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            r#"INSERT INTO cache_entries (key,method,url,status,headers,body,expires_at,validators,created_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
               ON CONFLICT(key) DO UPDATE SET status=excluded.status, headers=excluded.headers,
                 body=excluded.body, expires_at=excluded.expires_at, validators=excluded.validators,
                 created_at=excluded.created_at"#,
            params![
                entry.key, entry.method, entry.url, entry.status, entry.headers,
                entry.body, entry.expires_at, entry.validators, entry.created_at
            ],
        )?;
        Ok(())
    }

    pub fn cache_get(&self, key: &str) -> Result<Option<CacheEntry>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                r#"SELECT key,method,url,status,headers,body,expires_at,validators,created_at
                   FROM cache_entries WHERE key=?1"#,
                params![key],
                |r| {
                    Ok(CacheEntry {
                        key: r.get(0)?,
                        method: r.get(1)?,
                        url: r.get(2)?,
                        status: r.get(3)?,
                        headers: r.get(4)?,
                        body: r.get(5)?,
                        expires_at: r.get(6)?,
                        validators: r.get(7)?,
                        created_at: r.get(8)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn append_history(&self, row: &HistoryRow) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            r#"INSERT INTO history (ts,url,title,kind,session_id,duration_ms,ok)
               VALUES (?1,?2,?3,?4,?5,?6,?7)"#,
            params![
                row.ts,
                row.url,
                row.title,
                row.kind,
                row.session_id,
                row.duration_ms,
                row.ok as i64
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_history(&self, q: Option<&str>, limit: usize) -> Result<Vec<HistoryRow>> {
        let conn = self.lock();
        let limit = limit as i64;
        let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<HistoryRow> {
            Ok(HistoryRow {
                id: r.get(0)?,
                ts: r.get(1)?,
                url: r.get(2)?,
                title: r.get(3)?,
                kind: r.get(4)?,
                session_id: r.get(5)?,
                duration_ms: r.get(6)?,
                ok: r.get::<_, i64>(7)? != 0,
            })
        };
        let rows = if let Some(q) = q {
            let like = format!("%{}%", q.replace('%', ""));
            let mut stmt = conn.prepare(
                r#"SELECT id,ts,url,title,kind,session_id,duration_ms,ok FROM history
                   WHERE url LIKE ?1 OR title LIKE ?1 ORDER BY ts DESC, id DESC LIMIT ?2"#,
            )?;
            let it = stmt.query_map(params![like, limit], map)?;
            it.collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            let mut stmt = conn.prepare(
                r#"SELECT id,ts,url,title,kind,session_id,duration_ms,ok FROM history
                   ORDER BY ts DESC, id DESC LIMIT ?1"#,
            )?;
            let it = stmt.query_map(params![limit], map)?;
            it.collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    pub fn append_log(&self, row: &LogRow) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            r#"INSERT INTO logs (ts,level,target,message,fields) VALUES (?1,?2,?3,?4,?5)"#,
            params![row.ts, row.level, row.target, row.message, row.fields],
        )?;
        Ok(())
    }

    pub fn prune(&self, now: i64) -> Result<PruneReport> {
        let conn = self.lock();
        // 缓存按自己的过期时间清，不再额外多留 7 天。
        let cache_deleted = conn.execute(
            "DELETE FROM cache_entries WHERE expires_at < ?1",
            params![now],
        )? as i64;
        let history_deleted = conn.execute(
            "DELETE FROM history WHERE ts < ?1",
            params![now - HISTORY_RETENTION_SECS],
        )? as i64;
        let logs_deleted = conn.execute(
            "DELETE FROM logs WHERE ts < ?1",
            params![now - LOG_RETENTION_SECS],
        )? as i64;
        Ok(PruneReport {
            cache_deleted,
            history_deleted,
            logs_deleted,
        })
    }

    /// 各表行数，给 `cb db stats` 用。
    pub fn counts(&self) -> Result<Counts> {
        let conn = self.lock();
        let count = |table: &str| -> Result<i64> {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            Ok(conn.query_row(&sql, [], |r| r.get(0))?)
        };
        Ok(Counts {
            pages: count("pages")?,
            cache_entries: count("cache_entries")?,
            history: count("history")?,
            logs: count("logs")?,
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock();
        let v = conn
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub fn hash_url(url: &str) -> String {
    blake3::hash(url.as_bytes()).to_hex().to_string()
}

/// zstd 压缩（存储用）。
pub fn compress(data: &[u8]) -> Vec<u8> {
    zstd::encode_all(data, 3).unwrap_or_else(|_| data.to_vec())
}

/// zstd 解压（存储用）；失败时原样返回，兼容未压缩数据。
pub fn decompress(data: &[u8]) -> Vec<u8> {
    zstd::decode_all(data).unwrap_or_else(|_| data.to_vec())
}
