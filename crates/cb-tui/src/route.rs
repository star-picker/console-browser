//! 地址栏意图识别：一个框干四件事（URL / 搜索 / bang / cb: 命令）。

use cb_core::search::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(String),
    Search {
        engine: String,
        query: String,
    },
    Command(String),
    /// 界面内部动作（从地址栏也能触发，方便发现键位）。
    Internal(String),
}

/// 地址栏可用的内部动作名（cb: 后面与 ? 都能用）。
pub const INTERNAL_ACTIONS: &[&str] = &[
    "help",
    "history",
    "links",
    "palette",
    "trace",
    "ai",
    "reload",
    "back",
    "forward",
    "top",
    "bottom",
    "quit",
    "export-md",
    "export-html",
    "lang",
    "keys",
];

const DEFAULT_ENGINE: &str = "ddg";

fn looks_like_url(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    if lower.contains("://") {
        return true;
    }
    if lower.starts_with("localhost")
        || lower.starts_with("127.0.0.1")
        || lower.starts_with("[::1]")
    {
        return true;
    }
    let host = input.split('/').next().unwrap_or("");
    let has_dot = host.contains('.');
    let no_space = !input.chars().any(|c| c.is_whitespace());
    let tld_like = host
        .rsplit_once('.')
        .map(|(_, tld)| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
        .unwrap_or(false);
    has_dot && no_space && tld_like
}

fn normalize_url(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("://") {
        return input.to_string();
    }
    if lower.starts_with("localhost")
        || lower.starts_with("127.0.0.1")
        || lower.starts_with("[::1]")
    {
        return format!("http://{input}");
    }
    format!("https://{input}")
}

pub fn classify(input: &str) -> Target {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("cb:") {
        let name = rest.trim().to_lowercase();
        if INTERNAL_ACTIONS.contains(&name.as_str()) {
            return Target::Internal(name);
        }
        return Target::Command(rest.trim().to_string());
    }
    if trimmed == "?" || trimmed == "/?" {
        return Target::Internal("help".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix('!') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let prefix = parts.next().unwrap_or("");
        let query = parts.next().unwrap_or("").trim().to_string();
        if let Some(engine) = Engine::parse(prefix) {
            if !query.is_empty() {
                return Target::Search {
                    engine: engine.as_str().to_string(),
                    query,
                };
            }
        }
        return Target::Search {
            engine: DEFAULT_ENGINE.to_string(),
            query: rest.trim().to_string(),
        };
    }
    if looks_like_url(trimmed) {
        return Target::Url(normalize_url(trimmed));
    }
    Target::Search {
        engine: DEFAULT_ENGINE.to_string(),
        query: trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_detected_and_normalized() {
        assert_eq!(
            classify("https://example.com/a"),
            Target::Url("https://example.com/a".to_string())
        );
        assert_eq!(
            classify("example.com"),
            Target::Url("https://example.com".to_string())
        );
        assert_eq!(
            classify("localhost:8080/x"),
            Target::Url("http://localhost:8080/x".to_string())
        );
        assert_eq!(
            classify("127.0.0.1:3000"),
            Target::Url("http://127.0.0.1:3000".to_string())
        );
    }

    #[test]
    fn queries_go_to_default_engine() {
        assert_eq!(
            classify("rust http client"),
            Target::Search {
                engine: "ddg".to_string(),
                query: "rust http client".to_string()
            }
        );
    }

    #[test]
    fn bangs_pick_an_engine() {
        assert_eq!(
            classify("!gh ripgrep"),
            Target::Search {
                engine: "gh".to_string(),
                query: "ripgrep".to_string()
            }
        );
        assert_eq!(
            classify("!arxiv diffusion models"),
            Target::Search {
                engine: "arxiv".to_string(),
                query: "diffusion models".to_string()
            }
        );
        assert_eq!(
            classify("! just a query"),
            Target::Search {
                engine: "ddg".to_string(),
                query: "just a query".to_string()
            }
        );
    }

    #[test]
    fn cb_prefix_is_a_command() {
        assert_eq!(
            classify("cb:history"),
            Target::Internal("history".to_string())
        );
        assert_eq!(
            classify("cb: export https://a/ --to html"),
            Target::Command("export https://a/ --to html".to_string())
        );
    }
}
