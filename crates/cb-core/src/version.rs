//! 版本号结构：xx.yy.zz.YYYYMMDD.<gitshort>（构建时注入）。
//!
//! xx.yy.zz 来自 Cargo.toml；YYYYMMDD 与 hash 由 build.rs 从 git 取，
//! 这样任何一个二进制都能直接对上它对应的提交。

/// 完整版本串，例如 `0.1.0.20260214.a1b2c3d`。
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    ".",
    env!("CB_BUILD_DATE"),
    ".",
    env!("CB_BUILD_COMMIT")
);

/// 版本串的数字前缀（xx.yy.zz），用于 semver 比较。
pub const SEMVER: &str = env!("CARGO_PKG_VERSION");

/// 构建时注入的提交短 hash。
pub const COMMIT: &str = env!("CB_BUILD_COMMIT");

/// 构建时注入的日期 YYYYMMDD。
pub const BUILD_DATE: &str = env!("CB_BUILD_DATE");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_has_expected_shape() {
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 5, "xx.yy.zz.YYYYMMDD.hash, got {VERSION}");
        assert_eq!(parts[3].len(), 8, "date must be YYYYMMDD, got {}", parts[3]);
        assert!(
            parts[3].starts_with("20"),
            "date should look like a year, got {}",
            parts[3]
        );
        assert!(!parts[4].is_empty(), "commit hash must be present");
        assert!(parts[4].chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn semver_prefix_matches_cargo() {
        assert!(VERSION.starts_with(SEMVER));
    }
}
