//! 日志文件：jsonl + 轮转 + 脱敏。
//!
//! 设计取舍：屏幕/JSON 输出保持干净（日志绝不混进 stdout），
//! 需要留痕的东西写文件；写之前统一脱敏，避免 cookie/token 落盘。

use std::io::Write;
use std::path::{Path, PathBuf};

/// 日志目录：`%APPDATA%/console-browser/logs` 或 `$HOME/.local/state/console-browser/logs`。
pub fn logs_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("console-browser").join("logs");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("console-browser")
                .join("logs");
        }
    }
    PathBuf::from(".cb-logs")
}

/// 脱敏：把 cookie / authorization / key / token 后面的值替换成 ***。
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut i = 0usize;
    let needles = [
        "cookie: ",
        "authorization: ",
        "set-cookie: ",
        "api_key=",
        "apikey=",
        "token=",
        "key=",
    ];
    while i < text.len() {
        let rest = &lower[i..];
        let mut hit: Option<(usize, usize)> = None;
        for needle in needles {
            if let Some(pos) = rest.find(needle) {
                let key_end = i + pos + needle.len();
                // 头字段（Cookie/Authorization）整行脱敏；query 参数只脱到分隔符。
                let header_like = needle.contains("cookie") || needle.contains("authorization");
                let seps: &[char] = if header_like {
                    &['\n', '\r']
                } else {
                    &[' ', '\n', '\r', '&', ';', '"']
                };
                let value_end = text[key_end..]
                    .find(seps)
                    .map(|n| key_end + n)
                    .unwrap_or(text.len());
                hit = Some((value_end, key_end));
                break;
            }
        }
        match hit {
            Some((value_end, key_end)) => {
                out.push_str(&text[i..key_end.min(text.len())]);
                out.push_str("***");
                i = value_end;
            }
            None => {
                let ch = text[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

fn file_for_today(dir: &Path) -> PathBuf {
    dir.join(format!("cb-{}.jsonl", crate::now_secs() / 86_400))
}

fn rotate_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<()> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    if meta.len() <= max_bytes {
        return Ok(());
    }
    let rotated = path.with_extension("jsonl.1");
    let _ = std::fs::remove_file(&rotated);
    std::fs::rename(path, rotated)
}

/// 追加一条 jsonl（自动建目录、按 2MB 轮转到 `.1`）。返回写入路径。
pub fn append(line: &str) -> std::io::Result<PathBuf> {
    append_in(&logs_dir(), line, 2 * 1024 * 1024)
}

pub fn append_in(dir: &Path, line: &str, max_bytes: u64) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = file_for_today(dir);
    rotate_if_needed(&path, max_bytes)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", redact(line))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_values() {
        let raw = "Cookie: sid=abc123; theme=dark Authorization: Bearer sekret token=xyz";
        let out = redact(raw);
        assert!(!out.contains("abc123"), "{out}");
        assert!(!out.contains("sekret"), "{out}");
        assert!(!out.contains("xyz"), "{out}");
        assert!(out.contains("***"), "{out}");
    }

    #[test]
    fn appends_and_rotates() {
        let dir = std::env::temp_dir().join(format!("cb-logs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..3 {
            append_in(&dir, &format!("{{\"n\":{i}}}",), 10).expect("append");
        }
        let rotated = std::fs::read_dir(&dir)
            .expect("dir")
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".jsonl.1"));
        assert!(rotated, "rotation should produce .jsonl.1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
