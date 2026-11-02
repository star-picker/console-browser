//! 输出层：JSON 信封 vs 人读文本；退出码统一在这里定。

use cb_core::error::CbError;
use cb_core::{Envelope, Meta, ENGINE};

pub fn meta(elapsed_ms: u64, cached: bool, profile: &str) -> Meta {
    Meta {
        engine: ENGINE.to_string(),
        elapsed_ms,
        cached,
        profile: profile.to_string(),
    }
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
