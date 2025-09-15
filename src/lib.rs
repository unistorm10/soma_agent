use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

pub mod backends;
pub mod mcp;
#[cfg(feature = "sandboxed_exec")]
pub mod tools;

/// Ask represents a unit of work sent to a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ask {
    pub op: String,
    pub input: Value,
    pub context: Value,
}

/// Reply captures the outcome of a provider invocation.
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub output: Value,
    pub latency_ms: u64,
    pub cost: Value,
}

/// ProviderKind enumerates the deployment form of a provider.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderKind {
    Embedded,
    SidecarUds,
    RemoteGrpc,
}

/// ReasoningMode selects between direct and thinking-style execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReasoningMode {
    Direct,
    Reasoned,
}

impl ReasoningMode {
    fn as_str(&self) -> &'static str {
        match self {
            ReasoningMode::Direct => "direct",
            ReasoningMode::Reasoned => "reasoned",
        }
    }
}

/// ReasoningPolicy scores an input and picks a reasoning mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReasoningPolicy {
    pub threshold: usize,
    pub tool_weight: usize,
}

impl Default for ReasoningPolicy {
    fn default() -> Self {
        Self {
            threshold: 200,
            tool_weight: 50,
        }
    }
}

impl ReasoningPolicy {
    pub fn decide(&self, input: &Value, tool_count: usize) -> ReasoningMode {
        let text = input
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| input.to_string());
        let score = text.chars().count() + tool_count * self.tool_weight;
        if score > self.threshold {
            ReasoningMode::Reasoned
        } else {
            ReasoningMode::Direct
        }
    }
}

/// Provider is the universal interface for all execution modules.
pub trait Provider {
    fn kind(&self) -> ProviderKind;
    fn ask(&self, ask: Ask) -> Reply;
}

pub enum ToolSpec {
    Provider(Box<dyn Provider>),
    McpEndpoint(String),
    McpConfigFile(PathBuf),
}

impl<T: Provider + 'static> From<T> for ToolSpec {
    fn from(p: T) -> Self {
        ToolSpec::Provider(Box::new(p))
    }
}

impl From<String> for ToolSpec {
    fn from(url: String) -> Self {
        ToolSpec::McpEndpoint(url)
    }
}

impl From<PathBuf> for ToolSpec {
    fn from(p: PathBuf) -> Self {
        ToolSpec::McpConfigFile(p)
    }
}

async fn call_with_retry<F>(mut op: F, max_retries: usize, token: CancellationToken) -> Reply
where
    F: FnMut() -> Reply,
{
    let mut delay = Duration::from_millis(50);
    for attempt in 0..max_retries {
        if token.is_cancelled() {
            return Reply {
                ok: false,
                output: json!({"error": "cancelled"}),
                latency_ms: 0,
                cost: json!({}),
            };
        }
        let reply = op();
        if reply.ok || attempt + 1 == max_retries {
            return reply;
        }
        tokio::select! {
            _ = sleep(delay) => { delay *= 2; }
            _ = token.cancelled() => {
                return Reply {
                    ok: false,
                    output: json!({"error": "cancelled"}),
                    latency_ms: 0,
                    cost: json!({}),
                };
            }
        }
    }
    unreachable!()
}

/// Agent orchestrates calls to a provider with a simple step limit.
pub struct Agent<P: Provider> {
    provider: P,
    tools: HashMap<String, Box<dyn Provider>>,
    max_steps: usize,
    policy: ReasoningPolicy,
    max_tokens: usize,
    max_retries: usize,
    cancel_token: CancellationToken,
}

impl<P: Provider> Agent<P> {
    pub fn new(
        provider: P,
        max_steps: usize,
        max_tokens: usize,
        max_retries: usize,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            provider,
            tools: HashMap::new(),
            max_steps,
            policy: ReasoningPolicy::default(),
            max_tokens,
            max_retries,
            cancel_token,
        }
    }

    pub fn with_policy(
        provider: P,
        max_steps: usize,
        max_tokens: usize,
        policy: ReasoningPolicy,
        max_retries: usize,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            provider,
            tools: HashMap::new(),
            max_steps,
            policy,
            max_tokens,
            max_retries,
            cancel_token,
        }
    }

    pub fn register_tool<S, T>(
        &mut self,
        name: S,
        spec: T,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        S: Into<String>,
        T: Into<ToolSpec>,
    {
        let name = name.into();
        match spec.into() {
            ToolSpec::Provider(p) => {
                self.tools.insert(name, p);
            }
            ToolSpec::McpEndpoint(url) => {
                let provider = crate::mcp::McpProvider::new(url)?;
                self.tools.insert(name, Box::new(provider));
            }
            ToolSpec::McpConfigFile(path) => {
                let text = fs::read_to_string(path)?;
                let map: HashMap<String, String> = serde_json::from_str(&text)?;
                for (tool_name, url) in map {
                    let provider = crate::mcp::McpProvider::new(url)?;
                    self.tools.insert(tool_name, Box::new(provider));
                }
            }
        }
        Ok(())
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn call_tool(&self, name: &str, ask: Ask) -> Option<Reply> {
        self.tools.get(name).map(|p| p.ask(ask))
    }

    /// Runs the agent until the provider returns `ok` or the step or token limit is hit.
    pub async fn run(&self, ask: Ask) -> Reply {
        let mut remaining = self.max_tokens;
        let ask_tokens = estimate_tokens(&ask.input) + estimate_tokens(&ask.context);
        if ask_tokens > remaining {
            return Reply {
                ok: false,
                output: json!({"error": "token budget exceeded"}),
                latency_ms: 0,
                cost: json!({}),
            };
        }
        remaining -= ask_tokens;
        let mode = if ask_tokens * 100 / self.max_tokens > 85 {
            ReasoningMode::Direct
        } else {
            self.policy.decide(&ask.input, 0)
        };
        let mut current = Ask {
            context: json!({"reasoning": mode.as_str()}),
            ..ask
        };
        for step in 0..self.max_steps {
            let reply = call_with_retry(
                || self.provider.ask(current.clone()),
                self.max_retries,
                self.cancel_token.clone(),
            )
            .await;
            if self.cancel_token.is_cancelled() {
                return reply;
            }
            let reply_tokens = estimate_tokens(&reply.output);
            if reply_tokens > remaining {
                return Reply {
                    ok: false,
                    output: json!({"error": "token budget exceeded"}),
                    latency_ms: reply.latency_ms,
                    cost: reply.cost,
                };
            }
            remaining -= reply_tokens;
            if reply.ok {
                return reply;
            }
            if let Some(tool_calls) = reply.output.get("tool_calls").and_then(|v| v.as_array()) {
                if tool_calls.len() == 1 {
                    let tc = &tool_calls[0];
                    let name = tc["op"].as_str().unwrap_or("");
                    let input = tc["input"].clone();
                    if let Some(tool) = self.tools.get(name) {
                        let tool_tokens = estimate_tokens(&input);
                        if tool_tokens > remaining {
                            return Reply {
                                ok: false,
                                output: json!({"error": "token budget exceeded"}),
                                latency_ms: 0,
                                cost: json!({}),
                            };
                        }
                        remaining -= tool_tokens;
                        let name_owned = name.to_string();
                        let input_clone = input.clone();
                        let tool_ref = tool.as_ref();
                        let tool_reply = call_with_retry(
                            move || {
                                tool_ref.ask(Ask {
                                    op: name_owned.clone(),
                                    input: input_clone.clone(),
                                    context: json!({}),
                                })
                            },
                            self.max_retries,
                            self.cancel_token.clone(),
                        )
                        .await;
                        if self.cancel_token.is_cancelled() {
                            return tool_reply;
                        }
                        if !tool_reply.ok {
                            return Reply {
                                ok: false,
                                output: json!({
                                    "error": "tool invocation failed",
                                    "tool": name,
                                    "detail": tool_reply.output,
                                }),
                                latency_ms: tool_reply.latency_ms,
                                cost: tool_reply.cost,
                            };
                        }
                        let tool_reply_tokens = estimate_tokens(&tool_reply.output);
                        if tool_reply_tokens > remaining {
                            return Reply {
                                ok: false,
                                output: json!({"error": "token budget exceeded"}),
                                latency_ms: 0,
                                cost: json!({}),
                            };
                        }
                        remaining -= tool_reply_tokens;
                        current = Ask {
                            op: current.op.clone(),
                            input: tool_reply.output,
                            context: json!({
                                "reasoning": mode.as_str(),
                                "tool": name,
                            }),
                        };
                        let next_tokens =
                            estimate_tokens(&current.input) + estimate_tokens(&current.context);
                        if next_tokens > remaining {
                            return Reply {
                                ok: false,
                                output: json!({"error": "token budget exceeded"}),
                                latency_ms: 0,
                                cost: json!({}),
                            };
                        }
                        remaining -= next_tokens;
                        continue;
                    } else {
                        return Reply {
                            ok: false,
                            output: json!({"error": "unknown tool", "tool": name}),
                            latency_ms: 0,
                            cost: json!({}),
                        };
                    }
                } else if !tool_calls.is_empty() {
                    let mut names = Vec::new();
                    let mut futures = Vec::new();
                    for tc in tool_calls {
                        let name = tc["op"].as_str().unwrap_or("");
                        let input = tc["input"].clone();
                        let tool = match self.tools.get(name) {
                            Some(t) => t,
                            None => {
                                return Reply {
                                    ok: false,
                                    output: json!({"error": "unknown tool", "tool": name}),
                                    latency_ms: 0,
                                    cost: json!({}),
                                };
                            }
                        };
                        let tool_tokens = estimate_tokens(&input);
                        if tool_tokens > remaining {
                            return Reply {
                                ok: false,
                                output: json!({"error": "token budget exceeded"}),
                                latency_ms: 0,
                                cost: json!({}),
                            };
                        }
                        remaining -= tool_tokens;
                        names.push(name.to_string());
                        let name_owned = name.to_string();
                        let input_clone = input.clone();
                        let tool_ref = tool.as_ref();
                        let token = self.cancel_token.clone();
                        let max_r = self.max_retries;
                        futures.push(async move {
                            Ok::<Reply, ()>(
                                call_with_retry(
                                    move || {
                                        tool_ref.ask(Ask {
                                            op: name_owned.clone(),
                                            input: input_clone.clone(),
                                            context: json!({}),
                                        })
                                    },
                                    max_r,
                                    token,
                                )
                                .await,
                            )
                        });
                    }
                    let results = match futures.len() {
                        2 => {
                            let (r1, r2) =
                                tokio::try_join!(futures.remove(0), futures.remove(0)).unwrap();
                            vec![r1, r2]
                        }
                        3 => {
                            let (r1, r2, r3) = tokio::try_join!(
                                futures.remove(0),
                                futures.remove(0),
                                futures.remove(0)
                            )
                            .unwrap();
                            vec![r1, r2, r3]
                        }
                        _ => {
                            let mut outs = Vec::new();
                            for f in futures {
                                outs.push(f.await.unwrap());
                            }
                            outs
                        }
                    };
                    if self.cancel_token.is_cancelled() {
                        return Reply {
                            ok: false,
                            output: json!({"error": "cancelled"}),
                            latency_ms: 0,
                            cost: json!({}),
                        };
                    }
                    let mut outputs = Vec::new();
                    for (name, reply) in names.iter().zip(results.into_iter()) {
                        if !reply.ok {
                            return Reply {
                                ok: false,
                                output: json!({
                                    "error": "tool invocation failed",
                                    "tool": name,
                                    "detail": reply.output,
                                }),
                                latency_ms: reply.latency_ms,
                                cost: reply.cost,
                            };
                        }
                        let tool_reply_tokens = estimate_tokens(&reply.output);
                        if tool_reply_tokens > remaining {
                            return Reply {
                                ok: false,
                                output: json!({"error": "token budget exceeded"}),
                                latency_ms: 0,
                                cost: json!({}),
                            };
                        }
                        remaining -= tool_reply_tokens;
                        outputs.push(reply.output);
                    }
                    current = Ask {
                        op: current.op.clone(),
                        input: Value::Array(outputs),
                        context: json!({
                            "reasoning": mode.as_str(),
                            "tools": names,
                        }),
                    };
                    let next_tokens =
                        estimate_tokens(&current.input) + estimate_tokens(&current.context);
                    if next_tokens > remaining {
                        return Reply {
                            ok: false,
                            output: json!({"error": "token budget exceeded"}),
                            latency_ms: 0,
                            cost: json!({}),
                        };
                    }
                    remaining -= next_tokens;
                    continue;
                }
            }
            // propagate failure output into the next ask context
            current = Ask {
                op: current.op.clone(),
                input: reply.output,
                context: json!({
                    "reasoning": mode.as_str(),
                    "retry": step + 1
                }),
            };
            let next_tokens = estimate_tokens(&current.input) + estimate_tokens(&current.context);
            if next_tokens > remaining {
                return Reply {
                    ok: false,
                    output: json!({"error": "token budget exceeded"}),
                    latency_ms: 0,
                    cost: json!({}),
                };
            }
            remaining -= next_tokens;
        }
        Reply {
            ok: false,
            output: json!({"error": "step limit exceeded"}),
            latency_ms: 0,
            cost: json!({}),
        }
    }
}

fn estimate_tokens(value: &Value) -> usize {
    value.to_string().chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;
    use std::rc::Rc;
    use tokio_util::sync::CancellationToken;

    struct EchoProvider;

    impl Provider for EchoProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, ask: Ask) -> Reply {
            Reply {
                ok: true,
                output: ask.input,
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    #[test]
    fn echo_roundtrip() {
        let ask = Ask {
            op: "echo".into(),
            input: json!({"msg": "hi"}),
            context: json!({}),
        };
        let provider = EchoProvider;
        let reply = provider.ask(ask);
        assert!(reply.ok);
        assert_eq!(reply.output, json!({"msg": "hi"}));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_runs_until_ok() {
        let ask = Ask {
            op: "echo".into(),
            input: json!({"msg": "hi"}),
            context: json!({}),
        };
        let agent = Agent::new(EchoProvider, 3, 1000, 3, CancellationToken::new());
        let reply = agent.run(ask).await;
        assert!(reply.ok);
        assert_eq!(reply.output, json!({"msg": "hi"}));
    }

    struct FailProvider;

    impl Provider for FailProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, _ask: Ask) -> Reply {
            Reply {
                ok: false,
                output: json!({"error": "fail"}),
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_respects_step_limit() {
        let ask = Ask {
            op: "fail".into(),
            input: json!({}),
            context: json!({}),
        };
        let agent = Agent::new(FailProvider, 2, 1000, 3, CancellationToken::new());
        let reply = agent.run(ask).await;
        assert!(!reply.ok);
        assert_eq!(reply.output, json!({"error": "step limit exceeded"}));
    }

    #[test]
    fn reasoning_policy_scores_inputs() {
        let policy = ReasoningPolicy::default();
        assert_eq!(policy.decide(&json!("short"), 0), ReasoningMode::Direct);
        let long = "x".repeat(300);
        assert_eq!(policy.decide(&json!(long), 0), ReasoningMode::Reasoned);
    }

    struct InspectProvider;

    impl Provider for InspectProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, ask: Ask) -> Reply {
            Reply {
                ok: true,
                output: ask.context,
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_attaches_reasoning_mode() {
        let ask = Ask {
            op: "inspect".into(),
            input: json!("hello"),
            context: json!({}),
        };
        let agent = Agent::new(InspectProvider, 1, 1000, 3, CancellationToken::new());
        let reply = agent.run(ask).await;
        assert_eq!(reply.output["reasoning"], "direct");
    }

    struct ReasoningEcho;

    impl Provider for ReasoningEcho {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, ask: Ask) -> Reply {
            Reply {
                ok: true,
                output: ask.context["reasoning"].clone(),
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    struct BigProvider;

    impl Provider for BigProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, _ask: Ask) -> Reply {
            let output = "x".repeat(200);
            Reply {
                ok: true,
                output: json!(output),
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_enforces_token_budget() {
        let ask = Ask {
            op: "big".into(),
            input: json!("hi"),
            context: json!({}),
        };
        let agent = Agent::new(BigProvider, 1, 50, 3, CancellationToken::new());
        let reply = agent.run(ask).await;
        assert!(!reply.ok);
        assert_eq!(reply.output, json!({"error": "token budget exceeded"}));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn budget_forces_direct_mode() {
        let long = "x".repeat(90);
        let ask = Ask {
            op: "inspect".into(),
            input: json!(long),
            context: json!({}),
        };
        let policy = ReasoningPolicy {
            threshold: 10,
            tool_weight: 50,
        };
        let agent = Agent::with_policy(ReasoningEcho, 1, 105, policy, 3, CancellationToken::new());
        let reply = agent.run(ask).await;
        assert_eq!(reply.output, json!("direct"));
    }

    struct FlakyProvider {
        attempts: Rc<Cell<usize>>,
        succeed_on: usize,
    }

    impl Provider for FlakyProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, _ask: Ask) -> Reply {
            let count = self.attempts.get() + 1;
            self.attempts.set(count);
            if count >= self.succeed_on {
                Reply {
                    ok: true,
                    output: json!({"done": true}),
                    latency_ms: 0,
                    cost: json!({}),
                }
            } else {
                Reply {
                    ok: false,
                    output: json!({"error": "flaky"}),
                    latency_ms: 0,
                    cost: json!({}),
                }
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_retries_until_success() {
        let attempts = Rc::new(Cell::new(0));
        let provider = FlakyProvider {
            attempts: attempts.clone(),
            succeed_on: 3,
        };
        let agent = Agent::new(provider, 5, 1000, 3, CancellationToken::new());
        let ask = Ask {
            op: "flaky".into(),
            input: json!({}),
            context: json!({}),
        };
        let reply = agent.run(ask).await;
        assert!(reply.ok);
        assert_eq!(attempts.get(), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancels_on_token() {
        let attempts = Rc::new(Cell::new(0));
        let provider = FlakyProvider {
            attempts: attempts.clone(),
            succeed_on: usize::MAX,
        };
        let token = CancellationToken::new();
        let agent = Agent::new(provider, 5, 1000, 5, token.clone());
        let ask = Ask {
            op: "never".into(),
            input: json!({}),
            context: json!({}),
        };
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token.cancel();
        });
        let reply = agent.run(ask).await;
        handle.await.unwrap();
        assert!(!reply.ok);
        assert_eq!(reply.output, json!({"error": "cancelled"}));
        assert_eq!(attempts.get(), 1);
    }
}
