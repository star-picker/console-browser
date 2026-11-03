//! cb-plugins: 插件清单、权限与加载。
//!
//! 设计取舍（与设计文档一致，但分两步落地）：
//! 1) 本 crate 先做「清单 + 权限 + 发现/安装」——这些是安全边界，必须先有；
//! 2) JS 宿主（extractor/challenge/command 钩子真正执行）与 cb-js 共用同一个
//!    QuickJS 引擎，在 cb-js 那一步接上。没有清单和权限，直接执行 JS 是不负责任的。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 插件被允许做的事。默认全禁，必须在 plugin.json 里显式声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// 发网络请求（走 cb-net，保留指纹与 cookie 语义）
    Net,
    /// 读页面正文 / DOM
    Extract,
    /// 参与挑战处理（可以读响应、给建议）
    Challenge,
    /// 注册 CLI 子命令
    Command,
}

impl Permission {
    pub fn parse(raw: &str) -> Option<Permission> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "net" => Some(Permission::Net),
            "extract" => Some(Permission::Extract),
            "challenge" => Some(Permission::Challenge),
            "command" => Some(Permission::Command),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Net => "net",
            Permission::Extract => "extract",
            Permission::Challenge => "challenge",
            Permission::Command => "command",
        }
    }
}

/// 抽取器钩子：先声明式（CSS 选择器），JS 钩子在 cb-js 接。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractorHook {
    #[serde(default)]
    pub selectors: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hooks {
    #[serde(default)]
    pub extract: Option<ExtractorHook>,
}

/// plugin.json 的 schema。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub hooks: Hooks,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub dir: String,
}

/// 校验 manifest：名字安全、版本非空、权限白名单。返回规范化后的权限字符串。
pub fn validate(manifest: &Manifest) -> Result<Vec<String>, String> {
    let name = manifest.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err("plugin name must be 1..=64 chars".to_string());
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !ok || name.starts_with('.') || name.contains("..") {
        return Err(format!("unsafe plugin name: {name}"));
    }
    if manifest.version.trim().is_empty() {
        return Err("plugin version is required".to_string());
    }
    let mut permissions = Vec::new();
    for raw in &manifest.permissions {
        let permission =
            Permission::parse(raw).ok_or_else(|| format!("unknown permission: {raw}"))?;
        permissions.push(permission.as_str().to_string());
    }
    Ok(permissions)
}

/// 默认插件目录：`%APPDATA%/console-browser/plugins`（或 $HOME/.config/...）。
pub fn default_plugin_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("console-browser")
        .join("plugins")
}

/// 从目录读所有插件（每个子目录一个 plugin.json）。坏插件不致命，记进错误列表。
pub fn discover(dir: &Path) -> (Vec<Plugin>, Vec<String>) {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (plugins, errors);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("plugin.json");
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(raw) => raw,
            Err(e) => {
                errors.push(format!("{}: {e}", manifest_path.display()));
                continue;
            }
        };
        let manifest: Manifest = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: bad manifest: {e}", manifest_path.display()));
                continue;
            }
        };
        match validate(&manifest) {
            Ok(permissions) => plugins.push(Plugin {
                name: manifest.name.trim().to_string(),
                version: manifest.version.trim().to_string(),
                description: manifest.description.trim().to_string(),
                permissions,
                dir: path.display().to_string(),
            }),
            Err(e) => errors.push(format!("{}{e}", path.display())),
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    (plugins, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, perms: &[&str]) -> Manifest {
        Manifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: "test".to_string(),
            permissions: perms.iter().map(|s| s.to_string()).collect(),
            hooks: Hooks::default(),
        }
    }

    #[test]
    fn validates_name_and_permissions() {
        assert!(validate(&manifest("wechat", &["extract"])).is_ok());
        assert!(validate(&manifest("../evil", &[])).is_err());
        assert!(validate(&manifest(".hidden", &[])).is_err());
        assert!(
            validate(&manifest("ok", &["root"])).is_err(),
            "unknown permission must fail closed"
        );
    }

    #[test]
    fn discovers_manifests_and_reports_bad_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let good = dir.path().join("wechat");
        std::fs::create_dir_all(&good).expect("mkdir");
        std::fs::write(
            good.join("plugin.json"),
            r##"{"name":"wechat","version":"1.0.0","permissions":["extract"],"hooks":{"extract":{"selectors":["#js_content"],"remove":[".qr"]}}}"##,
        )
        .expect("write");
        let bad = dir.path().join("bad");
        std::fs::create_dir_all(&bad).expect("mkdir");
        std::fs::write(bad.join("plugin.json"), "not json").expect("write");
        let (plugins, errors) = discover(dir.path());
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "wechat");
        assert_eq!(plugins[0].permissions, vec!["extract".to_string()]);
        assert_eq!(errors.len(), 1);
    }
}
