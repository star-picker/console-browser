//! 反爬挑战升级的最后一级：让模型看挑战页，给出下一步建议。
//! 默认关闭，只有显式 --ai-escalate 才会花 token。

use crate::ai;
use crate::error::CbError;
use crate::Ctx;

/// 喂给模型的挑战页最多截这么多字符，避免把整页塞进上下文。
pub const MAX_SNIPPET_CHARS: usize = 2000;

fn build_prompt(url: &str, body: &str) -> String {
    let snippet: String = body.chars().take(MAX_SNIPPET_CHARS).collect();
    format!(
        "A headless browser got blocked by an anti-bot challenge.\n\
URL: {url}\n\
Page body (truncated):\n{snippet}\n\n\
Answer in at most two sentences: what concrete next step should the client take \
(for example: which cookie to set, whether the block is IP-based, or that a human must log in)?\n\
Do not invent credentials."
    )
}

/// 问模型下一步怎么办；返回一句可直接拼进错误提示的建议。
pub async fn llm_advice(ctx: &Ctx, url: &str, body: &str) -> Result<String, CbError> {
    let prompt = build_prompt(url, body);
    // 本地没有模型 key 时，借调用方的模型（MCP sampling）；有 key 先用本地。
    if let Some(sampler) = &ctx.sampler {
        if ai::AiConfig::from_env().ensure_ready().is_err() {
            let text = sampler.ask(&prompt).await?;
            let advice: String = text.trim().chars().take(400).collect();
            if advice.is_empty() {
                return Err(CbError::other("sampler returned empty challenge advice"));
            }
            return Ok(advice);
        }
    }
    let completion = ai::complete(ctx, "challenge", &prompt, 200).await?;
    let advice: String = completion.text.trim().chars().take(400).collect();
    if advice.is_empty() {
        return Err(CbError::other("model returned empty challenge advice"));
    }
    Ok(advice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_is_bounded_and_carries_the_url() {
        let body = "x".repeat(10_000);
        let prompt = build_prompt("https://blocked.example/x", &body);
        assert!(prompt.contains("https://blocked.example/x"));
        assert!(prompt.contains("Do not invent credentials"));
        assert!(
            prompt.chars().count() < MAX_SNIPPET_CHARS + 800,
            "prompt must stay bounded, got {} chars",
            prompt.chars().count()
        );
    }
}
