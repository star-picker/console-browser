//! 能力表：一个能力只定义一次，CLI / MCP / agent 共用同一份 schema。

use serde::Serialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub summary: &'static str,
    /// fetch | query | meta
    pub kind: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
}

const CONTENT_SCHEMA: fn() -> Value = || {
    json!({
        "type": "object",
        "properties": {
            "url": {"type": "string"},
            "final_url": {"type": "string"},
            "status": {"type": "integer"},
            "title": {"type": "string"},
            "content": {"type": "string", "description": "markdown (default), html or text depending on format"},
            "tokens_estimate": {"type": "integer"},
            "truncated": {"type": "boolean"},
            "cached": {"type": "boolean"}
        }
    })
};

fn browse_url() -> ToolSpec {
    ToolSpec {
        name: "browse_url",
        summary: "取一个网页并返回 AI 可直接阅读的内容（默认 markdown），带缓存与指纹冒充。",
        kind: "fetch",
        input_schema: json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {"type": "string", "description": "http(s) URL"},
                "format": {"type": "string", "enum": ["md", "html", "text", "json"], "default": "md"},
                "max_tokens": {"type": "integer", "minimum": 1},
                "scope": {"type": "string", "description": "CSS selector to restrict extraction"},
                "fresh": {"type": "boolean", "default": false}
            }
        }),
        output_schema: CONTENT_SCHEMA(),
    }
}

fn history_search() -> ToolSpec {
    ToolSpec {
        name: "history_search",
        summary: "在本机浏览历史里搜 URL/标题，返回最近的条目。",
        kind: "query",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 20}
            }
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "entries": {"type": "array", "items": {"type": "object"}}
            }
        }),
    }
}

fn selftest_status() -> ToolSpec {
    ToolSpec {
        name: "selftest_status",
        summary: "自检：db / fs / net / tls / js / llm / cache / mcp 八项状态，机器可读。",
        kind: "meta",
        input_schema: json!({"type": "object", "properties": {}}),
        output_schema: json!({
            "type": "object",
            "properties": {
                "all_ok": {"type": "boolean"},
                "checks": {"type": "array", "items": {"type": "object"}}
            }
        }),
    }
}

fn search_web() -> ToolSpec {
    ToolSpec {
        name: "search_web",
        summary: "结构化网页搜索（ddg / bing / gh / arxiv），返回标题、链接与摘要。\nStructured web search (ddg / bing / gh / arxiv) returning title, url and snippet.",
        kind: "fetch",
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "engine": {"type": "string", "enum": ["ddg", "bing", "gh", "arxiv"], "default": "ddg"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}
            }
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "results": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "url": {"type": "string"},
                        "snippet": {"type": "string"},
                        "source": {"type": "string"}
                    }
                }}
            }
        }),
    }
}

pub fn registry() -> &'static [ToolSpec] {
    static REGISTRY: OnceLock<Vec<ToolSpec>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            vec![
                browse_url(),
                search_web(),
                history_search(),
                selftest_status(),
            ]
        })
        .as_slice()
}

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    registry().iter().find(|t| t.name == name)
}
