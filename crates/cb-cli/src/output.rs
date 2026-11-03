//! 输出层：JSON 信封 vs 人读文本；退出码统一在这里定。

use cb_core::error::CbError;
use cb_core::{Envelope, Meta, ENGINE};

pub fn meta(elapsed_ms: u64, cached: bool, profile: &str) -> Meta {
    Meta {
        engine: ENGINE.to_string(),
        elapsed_ms,
        cached,
        cache: cb_core::CacheMeta {
            hit: cached,
            age_s: None,
            stale: false,
        },
        extract: None,
        profile: profile.to_string(),
    }
}

/// 填上真实的缓存年龄（秒）。meta 里放真值，别让调用方去 data 里猜。
pub fn with_cache(mut meta: Meta, age_s: Option<i64>) -> Meta {
    meta.cache.age_s = age_s.map(|s| s.max(0) as u64);
    meta
}

/// 附上抽取统计：字符数 / 段落数 / 容器得分，便于调用方判断正文质量。
pub fn with_extract(mut meta: Meta, outcome: &cb_core::FetchOutcome) -> Meta {
    let markdown = &outcome.markdown;
    let paragraphs = markdown
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .count();
    meta.extract = Some(cb_core::ExtractMeta {
        chars: markdown.chars().count(),
        paragraphs,
        container_score: outcome.container_score,
    });
    meta
}

pub fn print_json(envelope: &Envelope) {
    match serde_json::to_string_pretty(envelope) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("cb: failed to serialize envelope: {e}"),
    }
}

pub fn report_error(err: &CbError, json: bool, elapsed_ms: u64, profile: &str) {
    if json {
        print_json(&Envelope::err(meta(elapsed_ms, false, profile), err));
    } else {
        eprintln!("cb: {}: {}", err.body.code, err.body.message);
        if let Some(hint) = &err.body.needs_user {
            eprintln!("cb: needs you: [{}] {}", hint.kind, hint.hint);
        }
    }
}
