//! AI 能力：provider 解析、调用、出账、预算闸。
//! 每次模型调用都必须落账，超预算直接拒绝（退出码 9）。

use crate::error::CbError;
use crate::Ctx;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Deepseek,
    OpenAi,
    Ollama,
}

impl Provider {
    pub fn parse(raw: &str) -> Option<Provider> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "deepseek" | "ds" => Some(Provider::Deepseek),
            "openai" | "openai-compatible" | "compatible" | "oai" => Some(Provider::OpenAi),
            "ollama" | "local" => Some(Provider::Ollama),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Deepseek => "deepseek",
            Provider::OpenAi => "openai",
            Provider::Ollama => "ollama",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Deepseek => "https://api.deepseek.com",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Ollama => "http://127.0.0.1:11434/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Deepseek => "deepseek-chat",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Ollama => "qwen2.5",
        }
    }

    /// 密钥只从环境变量读，绝不进库、不进仓库。
    pub fn key_env(self) -> &'static [&'static str] {
        match self {
            Provider::Deepseek => &["DEEPSEEK_API_KEY", "CB_AI_KEY"],
            Provider::OpenAi => &["OPENAI_API_KEY", "CB_AI_KEY"],
            Provider::Ollama => &["CB_AI_KEY"],
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Provider::Ollama)
    }
}

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub daily_budget: Option<i64>,
    pub timeout_ms: u64,
    /// 测试用：从 cassette 回放请求。
    pub replay: Option<PathBuf>,
}

impl AiConfig {
    pub fn from_env() -> AiConfig {
        let provider = std::env::var("CB_AI_PROVIDER")
            .ok()
            .and_then(|raw| Provider::parse(&raw))
            .unwrap_or(Provider::Deepseek);
        let base_url = std::env::var("CB_AI_BASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| provider.default_base_url().to_string());
        let model = std::env::var("CB_AI_MODEL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| provider.default_model().to_string());
        let api_key = provider
            .key_env()
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .filter(|v| !v.trim().is_empty());
        let daily_budget = std::env::var("CB_AI_DAILY_BUDGET")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v > 0);
        let timeout_ms = std::env::var("CB_AI_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(60_000);
        AiConfig {
            provider,
            base_url,
            model,
            api_key,
            daily_budget,
            timeout_ms,
            replay: std::env::var("CB_AI_REPLAY").ok().map(PathBuf::from),
        }
    }

    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// 给 `cb ai config` 用：不打印密钥本体，只说有没有、从哪来。
    pub fn describe(&self) -> serde_json::Value {
        serde_json::json!({
            "provider": self.provider.as_str(),
            "base_url": self.base_url,
            "model": self.model,
            "api_key_present": self.api_key.is_some(),
            "api_key_env": self.provider.key_env(),
            "daily_budget": self.daily_budget,
            "timeout_ms": self.timeout_ms,
        })
    }

    pub fn ensure_ready(&self) -> Result<(), CbError> {
        if self.provider.needs_key() && self.api_key.is_none() {
            return Err(CbError::new_local(
                "ai_not_configured",
                format!(
                    "no API key found; set one of: {}",
                    self.provider.key_env().join(", ")
                ),
                "export DEEPSEEK_API_KEY=... then retry",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    pub text: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub model: String,
    pub provider: String,
    pub latency_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReply {
    pub text: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub model: String,
}

/// 解析 OpenAI 兼容的 chat completions 响应。
pub fn parse_chat_response(body: &str) -> Result<ChatReply, CbError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| CbError::other(format!("model returned invalid json: {e}")))?;
    if let Some(err) = value.get("error") {
        return Err(CbError::other(format!("model error: {err}")));
    }
    let text = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| CbError::other("model returned no content".to_string()))?
        .to_string();
    Ok(ChatReply {
        text,
        prompt_tokens: value["usage"]["prompt_tokens"].as_i64().unwrap_or(0),
        completion_tokens: value["usage"]["completion_tokens"].as_i64().unwrap_or(0),
        model: value["model"].as_str().unwrap_or_default().to_string(),
    })
}

pub fn today() -> i64 {
    crate::now_secs() / 86_400
}

/// 跑一次模型调用：预算闸 → 请求 → 解析 → 出账（成功失败都记）。
pub async fn complete(
    ctx: &Ctx,
    purpose: &str,
    prompt: &str,
    max_tokens: u64,
) -> Result<Completion, CbError> {
    let config = AiConfig::from_env();
    complete_with(ctx, &config, purpose, prompt, max_tokens).await
}

pub async fn complete_with(
    ctx: &Ctx,
    config: &AiConfig,
    purpose: &str,
    prompt: &str,
    max_tokens: u64,
) -> Result<Completion, CbError> {
    config.ensure_ready()?;
    let day = today();

    if let Some(budget) = config.daily_budget {
        let used = ctx
            .store
            .ai_tokens_used(day)
            .map_err(crate::error::store_err)?;
        if used >= budget {
            return Err(CbError::budget(format!(
                "daily token budget exhausted: {used}/{budget}"
            )));
        }
    }

    let started = std::time::Instant::now();
    let body = serde_json::json!({
        "model": config.model,
        "max_tokens": max_tokens,
        "stream": false,
        "messages": [{ "role": "user", "content": prompt }],
    })
    .to_string();

    let mut request = cb_net::Request::post(config.chat_url(), body);
    if let Some(key) = &config.api_key {
        request = request.header("authorization", &format!("Bearer {key}"));
    }
    let client = match &config.replay {
        Some(path) => cb_net::NetClient::replay(path)?,
        None => cb_net::NetClient::new(ctx.profile)?,
    };

    let response = match tokio::time::timeout(
        std::time::Duration::from_millis(config.timeout_ms),
        client.get(&request),
    )
    .await
    {
        Err(_) => Err(CbError::timeout(format!(
            "model did not answer within {}ms",
            config.timeout_ms
        ))),
        Ok(Err(e)) => Err(e.into()),
        Ok(Ok(resp)) if resp.status >= 400 => Err(CbError::network(format!(
            "model http {}: {}",
            resp.status,
            resp.text().chars().take(300).collect::<String>()
        ))),
        Ok(Ok(resp)) => Ok(resp),
    };

    let latency_ms = started.elapsed().as_millis() as i64;

    match response {
        Ok(resp) => {
            let parsed = parse_chat_response(&resp.text());
            match parsed {
                Ok(reply) => {
                    let _ = ctx.store.record_llm_call(&cb_store::LlmCall {
                        ts: crate::now_secs(),
                        day,
                        provider: config.provider.as_str().to_string(),
                        model: if reply.model.is_empty() {
                            config.model.clone()
                        } else {
                            reply.model.clone()
                        },
                        purpose: purpose.to_string(),
                        prompt_tokens: reply.prompt_tokens,
                        completion_tokens: reply.completion_tokens,
                        latency_ms,
                        ok: true,
                        error: None,
                    });
                    Ok(Completion {
                        text: reply.text,
                        prompt_tokens: reply.prompt_tokens,
                        completion_tokens: reply.completion_tokens,
                        model: reply.model,
                        provider: config.provider.as_str().to_string(),
                        latency_ms,
                    })
                }
                Err(e) => {
                    let _ = ctx.store.record_llm_call(&cb_store::LlmCall {
                        ts: crate::now_secs(),
                        day,
                        provider: config.provider.as_str().to_string(),
                        model: config.model.clone(),
                        purpose: purpose.to_string(),
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        latency_ms,
                        ok: false,
                        error: Some(e.body.message.clone()),
                    });
                    Err(e)
                }
            }
        }
        Err(e) => {
            let _ = ctx.store.record_llm_call(&cb_store::LlmCall {
                ts: crate::now_secs(),
                day,
                provider: config.provider.as_str().to_string(),
                model: config.model.clone(),
                purpose: purpose.to_string(),
                prompt_tokens: 0,
                completion_tokens: 0,
                latency_ms,
                ok: false,
                error: Some(e.body.message.clone()),
            });
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_and_keys() {
        assert_eq!(Provider::parse("ds"), Some(Provider::Deepseek));
        assert_eq!(Provider::parse("compatible"), Some(Provider::OpenAi));
        assert!(Provider::parse("gpt5").is_none());
        assert_eq!(Provider::Deepseek.default_model(), "deepseek-chat");
        assert!(!Provider::Ollama.needs_key());
        assert!(Provider::Deepseek.key_env().contains(&"DEEPSEEK_API_KEY"));
    }

    #[test]
    fn parses_chat_completion_payload() {
        let body = r#"{"model":"deepseek-chat","choices":[{"message":{"role":"assistant","content":"hello"}}],"usage":{"prompt_tokens":11,"completion_tokens":3}}"#;
        let reply = parse_chat_response(body).expect("parse");
        assert_eq!(reply.text, "hello");
        assert_eq!(reply.prompt_tokens, 11);
        assert_eq!(reply.completion_tokens, 3);
    }

    #[test]
    fn surfaces_model_errors() {
        let body = r#"{"error":{"message":"insufficient balance"}}"#;
        let err = parse_chat_response(body).expect_err("must fail");
        assert!(err.body.message.contains("insufficient balance"));
    }

    #[test]
    fn refuses_without_key() {
        let config = AiConfig {
            provider: Provider::Deepseek,
            base_url: Provider::Deepseek.default_base_url().to_string(),
            model: Provider::Deepseek.default_model().to_string(),
            api_key: None,
            daily_budget: None,
            timeout_ms: 1000,
            replay: None,
        };
        let err = config.ensure_ready().expect_err("must fail");
        assert_eq!(err.body.code, "ai_not_configured");
        assert_eq!(err.exit_code(), crate::error::EXIT_OTHER);
    }

    #[test]
    fn describe_never_leaks_the_key() {
        let config = AiConfig {
            provider: Provider::Deepseek,
            base_url: "https://api.deepseek.com".to_string(),
            model: "deepseek-chat".to_string(),
            api_key: Some("sk-secret-value".to_string()),
            daily_budget: Some(1000),
            timeout_ms: 1000,
            replay: None,
        };
        let described = config.describe().to_string();
        assert!(
            !described.contains("sk-secret-value"),
            "key leaked: {described}"
        );
        assert!(described.contains("api_key_present") && described.contains("true"));
    }
}
#[cfg(test)]
mod wire_tests {
    use super::*;
    use cb_net::Profile;

    fn config(replay: std::path::PathBuf, budget: Option<i64>) -> AiConfig {
        AiConfig {
            provider: Provider::OpenAi,
            base_url: "https://ai.test/v1".to_string(),
            model: "test-model".to_string(),
            api_key: Some("sk-test-not-a-real-key".to_string()),
            daily_budget: budget,
            timeout_ms: 5_000,
            replay: Some(replay),
        }
    }

    fn cassette(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let path = dir.path().join("ai.cassette.json");
        let body = r#"{"model":"test-model","choices":[{"message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":7,"completion_tokens":2}}"#;
        let payload = serde_json::json!({
            "version": 1,
            "entries": [{
                "url": "https://ai.test/v1/chat/completions",
                "final_url": "https://ai.test/v1/chat/completions",
                "status": 200,
                "headers": {"content-type": "application/json"},
                "body": body,
            }]
        });
        std::fs::write(&path, serde_json::to_string_pretty(&payload).expect("json"))
            .expect("write");
        path
    }

    #[tokio::test]
    async fn successful_call_is_recorded_in_the_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = cb_store::Store::open_in_memory().expect("store");
        let ctx = crate::Ctx::new(store, Profile::Chrome);
        let cfg = config(cassette(&dir), None);

        let completion = complete_with(&ctx, &cfg, "test", "ping", 16)
            .await
            .expect("completion");
        assert_eq!(completion.text, "pong");
        assert_eq!(completion.prompt_tokens, 7);
        assert_eq!(completion.completion_tokens, 2);

        let used = ctx.store.ai_tokens_used(today()).expect("used");
        assert_eq!(used, 9, "ledger must count prompt+completion");
        let usage = ctx.store.llm_usage(7).expect("usage");
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].1, 7);
    }

    #[tokio::test]
    async fn budget_gate_refuses_before_calling_the_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = cb_store::Store::open_in_memory().expect("store");
        let ctx = crate::Ctx::new(store, Profile::Chrome);
        // 先记一笔把当天预算用光
        ctx.store
            .record_llm_call(&cb_store::LlmCall {
                ts: crate::now_secs(),
                day: today(),
                provider: "openai".to_string(),
                model: "test-model".to_string(),
                purpose: "seed".to_string(),
                prompt_tokens: 100,
                completion_tokens: 0,
                latency_ms: 1,
                ok: true,
                error: None,
            })
            .expect("seed");

        let cfg = config(cassette(&dir), Some(50));
        let err = complete_with(&ctx, &cfg, "test", "ping", 16)
            .await
            .expect_err("must refuse");
        assert_eq!(err.body.code, "budget_exhausted");
        assert_eq!(err.exit_code(), crate::error::EXIT_BUDGET);
    }
}
