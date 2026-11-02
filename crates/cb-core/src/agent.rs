//! 最小 agent 循环：模型出 JSON 动作 → 执行 → 观察 → 再决策。
//! 护栏：步数上限、token 上限、每步观察裁剪、全程留痕。

use crate::ai::{self, AiConfig};
use crate::error::CbError;
use crate::search::{self, Engine, SearchOpts};
use crate::{FetchOpts, Format};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentOpts {
    pub goal: String,
    pub max_steps: usize,
    /// 本任务累计 completion token 上限（0 = 不限）。
    pub max_tokens: i64,
    /// 单步观察最多保留多少字符，避免上下文吃 token。
    pub observation_chars: usize,
    pub replay: Option<PathBuf>,
}

impl AgentOpts {
    pub fn new(goal: impl Into<String>) -> AgentOpts {
        AgentOpts {
            goal: goal.into(),
            max_steps: 6,
            max_tokens: 0,
            observation_chars: 1200,
            replay: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub n: usize,
    pub action: String,
    pub input: String,
    pub observation: String,
    pub ok: bool,
    pub completion_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub goal: String,
    pub answer: String,
    pub steps: Vec<AgentStep>,
    pub completion_tokens: i64,
    pub finished: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
enum Action {
    Search {
        #[serde(default)]
        engine: Option<String>,
        query: String,
    },
    Fetch {
        url: String,
    },
    Answer {
        answer: String,
    },
}

const PROMPT_HEAD: &str =
    "You are a browsing agent. Reply with exactly ONE json object and nothing else.\n\
Allowed actions:\n\
{\"action\":\"search\",\"engine\":\"ddg|bing|gh|arxiv\",\"query\":\"...\"}\n\
{\"action\":\"fetch\",\"url\":\"https://...\"}\n\
{\"action\":\"answer\",\"answer\":\"...\"}\n";

fn build_prompt(opts: &AgentOpts, steps: &[AgentStep], remaining: usize) -> String {
    let mut history = String::new();
    for step in steps {
        history.push_str(&format!(
            "step {}: {} {} -> {}\n",
            step.n, step.action, step.input, step.observation
        ));
    }
    format!(
        "{PROMPT_HEAD}Goal: {}\nRemaining steps: {}\nSteps so far:\n{}\nReply with the next json action.",
        opts.goal,
        remaining,
        if history.is_empty() { "(none)" } else { &history }
    )
}

fn trimmed(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push_str(" …[truncated]");
    }
    out.replace('\n', " ")
}

async fn execute(
    ctx: &crate::Ctx,
    opts: &AgentOpts,
    action: &Action,
) -> Result<(String, String), CbError> {
    match action {
        Action::Search { engine, query } => {
            let engine = engine
                .as_deref()
                .and_then(Engine::parse)
                .unwrap_or(Engine::DuckDuckGo);
            let mut sopts = SearchOpts::new(engine, query.clone());
            sopts.limit = 8;
            sopts.replay = opts.replay.clone();
            let results = search::search(ctx, &sopts).await?;
            let mut text = String::new();
            for (i, r) in results.iter().enumerate() {
                text.push_str(&format!(
                    "{}. {} | {} | {}\n",
                    i + 1,
                    r.title,
                    r.url,
                    r.snippet
                ));
            }
            Ok((
                format!("{}:{}", engine.as_str(), query),
                trimmed(&text, opts.observation_chars),
            ))
        }
        Action::Fetch { url } => {
            let fopts = FetchOpts {
                url: url.clone(),
                format: Format::Md,
                max_tokens: Some(600),
                replay: opts.replay.clone(),
                ..Default::default()
            };
            let outcome = crate::fetch_page(ctx, &fopts).await?;
            Ok((
                url.clone(),
                trimmed(&outcome.markdown, opts.observation_chars),
            ))
        }
        Action::Answer { answer } => Ok(("answer".to_string(), answer.clone())),
    }
}

pub async fn run(ctx: &crate::Ctx, opts: &AgentOpts) -> Result<AgentRun, CbError> {
    if opts.goal.trim().is_empty() {
        return Err(CbError::usage("goal is required"));
    }
    let config = AiConfig::from_env();
    let mut steps: Vec<AgentStep> = Vec::new();
    let mut completion_tokens = 0i64;

    for n in 1..=opts.max_steps {
        let remaining = opts.max_steps - n + 1;
        let prompt = build_prompt(opts, &steps, remaining);
        let completion = ai::complete_with(ctx, &config, "agent", &prompt, 400).await?;
        completion_tokens += completion.completion_tokens;

        // 模型没按格式回：把整段当答案，别把用户卡死在循环里。
        let action = match serde_json::from_str::<Action>(completion.text.trim()) {
            Ok(action) => action,
            Err(_) => Action::Answer {
                answer: completion.text.trim().to_string(),
            },
        };

        let (input, observation) = execute(ctx, opts, &action).await?;
        let is_answer = matches!(action, Action::Answer { .. });
        steps.push(AgentStep {
            n,
            action: match &action {
                Action::Search { .. } => "search".to_string(),
                Action::Fetch { .. } => "fetch".to_string(),
                Action::Answer { .. } => "answer".to_string(),
            },
            input,
            observation: observation.clone(),
            ok: true,
            completion_tokens: completion.completion_tokens,
        });

        if is_answer {
            return Ok(AgentRun {
                goal: opts.goal.clone(),
                answer: observation,
                steps,
                completion_tokens,
                finished: true,
            });
        }

        if opts.max_tokens > 0 && completion_tokens >= opts.max_tokens {
            return Err(CbError::budget(format!(
                "agent token budget reached: {completion_tokens}/{}",
                opts.max_tokens
            )));
        }
    }

    Err(CbError::other(format!(
        "agent ran out of steps ({}) without an answer",
        opts.max_steps
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_carries_goal_and_history() {
        let opts = AgentOpts::new("find x");
        let steps = vec![AgentStep {
            n: 1,
            action: "search".to_string(),
            input: "gh:x".to_string(),
            observation: "result".to_string(),
            ok: true,
            completion_tokens: 3,
        }];
        let prompt = build_prompt(&opts, &steps, 4);
        assert!(prompt.contains("find x"));
        assert!(prompt.contains("Remaining steps: 4"));
        assert!(prompt.contains("gh:x"));
    }

    #[test]
    fn parses_actions_from_model_json() {
        let answer: Action =
            serde_json::from_str(r#"{"action":"answer","answer":"done"}"#).expect("answer");
        assert!(matches!(answer, Action::Answer { .. }));
        let search: Action =
            serde_json::from_str(r#"{"action":"search","query":"rust"}"#).expect("search");
        assert!(matches!(search, Action::Search { engine: None, .. }));
        let fetch: Action =
            serde_json::from_str(r#"{"action":"fetch","url":"https://e.com"}"#).expect("fetch");
        assert!(matches!(fetch, Action::Fetch { .. }));
    }

    #[test]
    fn truncated_observations_stay_short() {
        let long = "x".repeat(5000);
        let cut = trimmed(&long, 100);
        assert!(cut.chars().count() < 200);
        assert!(cut.contains("truncated"));
    }
}
