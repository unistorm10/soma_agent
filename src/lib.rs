use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

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

/// Agent orchestrates calls to a provider with a simple step limit.
pub struct Agent<P: Provider> {
    provider: P,
    tools: HashMap<String, Box<dyn Provider>>,
    max_steps: usize,
    policy: ReasoningPolicy,
    max_tokens: usize,
}

impl<P: Provider> Agent<P> {
    pub fn new(provider: P, max_steps: usize, max_tokens: usize) -> Self {
        Self {
            provider,
            tools: HashMap::new(),
            max_steps,
            policy: ReasoningPolicy::default(),
            max_tokens,
        }
    }

    pub fn with_policy(
        provider: P,
        max_steps: usize,
        max_tokens: usize,
        policy: ReasoningPolicy,
    ) -> Self {
        Self {
            provider,
            tools: HashMap::new(),
            max_steps,
            policy,
            max_tokens,
        }
    }

    pub fn register_tool<T: Provider + 'static>(&mut self, name: impl Into<String>, tool: T) {
        self.tools.insert(name.into(), Box::new(tool));
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
            let reply = self.provider.ask(current.clone());
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
                        let tool_reply = tool.ask(Ask {
                            op: name.to_string(),
                            input,
                            context: json!({}),
                        });
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
                                }
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
                        futures.push(async move {
                            Ok::<Reply, ()>(tool.ask(Ask {
                                op: name.to_string(),
                                input,
                                context: json!({}),
                            }))
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
        let agent = Agent::new(EchoProvider, 3, 1000);
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
        let agent = Agent::new(FailProvider, 2, 1000);
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
        let agent = Agent::new(InspectProvider, 1, 1000);
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
        let agent = Agent::new(BigProvider, 1, 50);
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
        let agent = Agent::with_policy(ReasoningEcho, 1, 105, policy);
        let reply = agent.run(ask).await;
        assert_eq!(reply.output, json!("direct"));
    }

    struct AddTool;

    impl Provider for AddTool {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, ask: Ask) -> Reply {
            let a = ask.input["a"].as_i64().unwrap();
            let b = ask.input["b"].as_i64().unwrap();
            Reply {
                ok: true,
                output: json!({"sum": a + b}),
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }

    struct ToolCallProvider {
        seen_tool: Cell<bool>,
    }

    impl Provider for ToolCallProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Embedded
        }

        fn ask(&self, ask: Ask) -> Reply {
            if self.seen_tool.get() {
                Reply {
                    ok: true,
                    output: json!({"final": ask.input["sum"].clone()}),
                    latency_ms: 0,
                    cost: json!({}),
                }
            } else {
                self.seen_tool.set(true);
                Reply {
                    ok: false,
                    output: json!({
                        "tool_calls": [
                            {"op": "adder", "input": {"a": 1, "b": 2}}
                        ]
                    }),
                    latency_ms: 0,
                    cost: json!({}),
                }
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_routes_tool_calls() {
        let ask = Ask {
            op: "sum".into(),
            input: json!({}),
            context: json!({}),
        };
        let mut agent = Agent::new(
            ToolCallProvider {
                seen_tool: Cell::new(false),
            },
            3,
            1000,
        );
        agent.register_tool("adder", AddTool);
        let reply = agent.run(ask).await;
        assert!(reply.ok);
        assert_eq!(reply.output, json!({"final": 3}));
    }
}
