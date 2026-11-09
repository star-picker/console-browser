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
    /// 生效域名（后缀匹配）；JSON 里也可写 "match"。
    #[serde(default, alias = "match")]
    pub match_hosts: Vec<String>,
    #[serde(default)]
    pub hooks: Hooks,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub match_hosts: Vec<String>,
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

/// 为某个 host 选一个抽取器插件：需要 extract 权限、声明了匹配域名、且有选择器。
/// 返回 (插件名, 选择器列表)。
pub fn extractor_for_host(dir: &Path, host: &str) -> Option<(String, Vec<String>)> {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let (plugins, _) = discover(dir);
    for plugin in plugins {
        if !plugin.permissions.iter().any(|p| p == "extract") {
            continue;
        }
        let matched = plugin
            .match_hosts
            .iter()
            .any(|suffix| host.ends_with(&suffix.trim().to_ascii_lowercase()));
        if !matched {
            continue;
        }
        let manifest = match read_manifest(&Path::new(&plugin.dir).join("plugin.json")) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let Some(hook) = manifest.hooks.extract else {
            continue;
        };
        let selectors: Vec<String> = hook
            .selectors
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if selectors.is_empty() {
            continue;
        }
        return Some((plugin.name, selectors));
    }
    None
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
        let manifest = match read_manifest(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: {e}", manifest_path.display()));
                continue;
            }
        };
        match validate(&manifest) {
            Ok(permissions) => plugins.push(Plugin {
                name: manifest.name.trim().to_string(),
                version: manifest.version.trim().to_string(),
                description: manifest.description.trim().to_string(),
                permissions,
                match_hosts: manifest.match_hosts.clone(),
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
            match_hosts: Vec::new(),
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
    fn extractor_for_host_requires_permission_and_match() {
        let dir = tempfile::tempdir().expect("tmp");
        let good = dir.path().join("wechat");
        std::fs::create_dir_all(&good).expect("mkdir");
        std::fs::write(
            good.join("plugin.json"),
            r##"{"name":"wechat","version":"1.0.0","permissions":["extract"],"match":["mp.weixin.qq.com"],"hooks":{"extract":{"selectors":["#js_content"]}}}"##,
        )
        .expect("write");
        let hit = extractor_for_host(dir.path(), "mp.weixin.qq.com").expect("hit");
        assert_eq!(hit.0, "wechat");
        assert_eq!(hit.1, vec!["#js_content".to_string()]);
        assert!(extractor_for_host(dir.path(), "example.com").is_none());

        // 没有 extract 权限就不生效（fail closed）
        let noperm = dir.path().join("noperm");
        std::fs::create_dir_all(&noperm).expect("mkdir");
        std::fs::write(
            noperm.join("plugin.json"),
            r##"{"name":"noperm","version":"1.0.0","permissions":[],"match":["noperm.example"],"hooks":{"extract":{"selectors":["#x"]}}}"##,
        )
        .expect("write");
        assert!(extractor_for_host(dir.path(), "noperm.example").is_none());
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
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 读 manifest：容忍 Windows 编辑器带的 UTF-8 BOM（否则 serde 在 column 1 就报错）。
fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw = raw.trim_start_matches('\u{feff}');
    serde_json::from_str(raw).map_err(|e| format!("bad manifest: {e}"))
}

/// 安装插件：先校验 manifest，再整目录复制到插件目录（同名覆盖）。
pub fn install(src: &Path, plugins_dir: &Path) -> Result<PathBuf, String> {
    let manifest = read_manifest(&src.join("plugin.json"))?;
    validate(&manifest)?;
    let dest = plugins_dir.join(manifest.name.trim());
    std::fs::create_dir_all(plugins_dir).map_err(|e| format!("create plugins dir: {e}"))?;
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("replace {}: {e}", dest.display()))?;
    }
    copy_dir(src, &dest).map_err(|e| format!("copy plugin: {e}"))?;
    Ok(dest)
}

#[cfg(test)]
mod install_tests {
    use super::*;

    #[test]
    fn installs_valid_plugin_and_rejects_bad_manifest() {
        let work = tempfile::tempdir().expect("tmp");
        let src = work.path().join("src");
        std::fs::create_dir_all(&src).expect("mkdir");
        std::fs::write(
            src.join("plugin.json"),
            r##"{"name":"demo","version":"1.0.0","permissions":["extract"],"hooks":{}}"##,
        )
        .expect("write");
        std::fs::write(src.join("extract.js"), "// x").expect("write");
        let plugins = work.path().join("plugins");
        let dest = install(&src, &plugins).expect("install");
        assert!(dest.join("plugin.json").exists());
        assert!(dest.join("extract.js").exists(), "files copied");
        let (found, errors) = discover(&plugins);
        assert_eq!(found.len(), 1);
        assert!(errors.is_empty());
        assert_eq!(found[0].name, "demo");

        let bad = work.path().join("bad");
        std::fs::create_dir_all(&bad).expect("mkdir");
        std::fs::write(bad.join("plugin.json"), "{\"name\":\"x\"}").expect("write");
        assert!(
            install(&bad, &plugins).is_err(),
            "missing version must be rejected"
        );
    }
}
