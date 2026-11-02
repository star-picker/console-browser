use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn main() {
    let hash = std::env::var("CB_BUILD_COMMIT")
        .ok()
        .or_else(|| git(&["rev-parse", "--short=7", "HEAD"]))
        .unwrap_or_else(|| "nogit".to_string());
    let date = std::env::var("CB_BUILD_DATE")
        .ok()
        .or_else(|| git(&["show", "-s", "--format=%cd", "--date=format:%Y%m%d", "HEAD"]))
        .unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // 粗略换算 UTC 日期（够用；精确日期由 git 提供）
            let days = now / 86_400;
            let (y, m, d) = civil_from_days(days as i64);
            format!("{y:04}{m:02}{d:02}")
        });
    println!("cargo:rustc-env=CB_BUILD_COMMIT={hash}");
    println!("cargo:rustc-env=CB_BUILD_DATE={date}");
    // 任何提交都会改 refs，所以盯 .git 目录本身，避免版本号停在旧提交。
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/main");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CB_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=CB_BUILD_DATE");
}

/// Howard Hinnant 的 civil_from_days。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
