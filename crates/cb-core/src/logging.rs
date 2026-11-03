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

/// 日志聚合：命令次数、错误码分布、平均耗时。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LogStats {
    pub lines: usize,
    pub errors: usize,
    pub avg_ms: u64,
    pub commands: std::collections::BTreeMap<String, usize>,
    pub error_codes: std::collections::BTreeMap<String, usize>,
}

/// 从 argv 里找真正的子命令：跳过全局旗标及其值（--db/--lang/--profile…）。
fn command_of(argv: &[serde_json::Value]) -> String {
    let mut i = 0usize;
    while i < argv.len() {
        let Some(arg) = argv[i].as_str() else {
            i += 1;
            continue;
        };
        if arg == "--db" || arg == "--lang" || arg == "--profile" || arg == "--timeout" {
            i += 2;
            continue;
        }
        if arg.starts_with('-') {
            i += 1;
            continue;
        }
        return arg.to_string();
    }
    "(none)".to_string()
}

/// 读目录下所有 jsonl 汇总。坏行跳过，不影响统计。
pub fn stats(dir: &Path) -> LogStats {
    let mut out = LogStats::default();
    let mut total_ms: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "jsonl").unwrap_or(true) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            out.lines += 1;
            total_ms += value
                .get("elapsed_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if let Some(code) = value.get("error").and_then(|v| v.as_str()) {
                out.errors += 1;
                *out.error_codes.entry(code.to_string()).or_default() += 1;
            }
            if let Some(argv) = value.get("argv").and_then(|v| v.as_array()) {
                *out.commands.entry(command_of(argv)).or_default() += 1;
            }
        }
    }
    if out.lines > 0 {
        out.avg_ms = total_ms / out.lines as u64;
    }
    out
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

#[test]
fn stats_aggregate_commands_and_errors() {
    let dir = std::env::temp_dir().join(format!("cb-logs-stats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
            dir.join("cb-1.jsonl"),
            concat!(
                "{\"argv\":[\"fetch\",\"https://a/\"],\"code\":0,\"elapsed_ms\":100,\"error\":null}\n",
                "{\"argv\":[\"fetch\",\"https://b/\"],\"code\":10,\"elapsed_ms\":300,\"error\":\"parse_failed\"}\n",
                "not json\n"
            ),
        )
        .expect("write");
    let s = stats(&dir);
    assert_eq!(s.lines, 2, "bad line must be skipped");
    assert_eq!(s.errors, 1);
    assert_eq!(s.commands.get("fetch"), Some(&2));
    assert_eq!(s.error_codes.get("parse_failed"), Some(&1));
    assert_eq!(s.avg_ms, 200);
    let _ = std::fs::remove_dir_all(&dir);
}
