use std::time::{Duration, Instant};

use crate::{Ask, Provider, ProviderKind, Reply};
use reqwest::blocking::Client;
use serde_json::{json, Value};

#[derive(Clone)]
pub struct HttpConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub timeout: Duration,
}

pub struct HttpProvider {
    config: HttpConfig,
    client: Client,
}

impl HttpProvider {
    pub fn new(config: HttpConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("http client");
        Self { config, client }
    }
}

impl Provider for HttpProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::RemoteGrpc
    }

    fn ask(&self, ask: Ask) -> Reply {
        let Ask {
            op: _,
            input,
            context,
        } = ask;
        let mut body = json!({
            "model": self.config.model,
            "messages": input,
        });

        let dialect = context
            .get("dialect")
            .and_then(|v| v.as_str())
            .unwrap_or("openai");

        if let Some(tools) = context.get("tools") {
            match dialect {
                "dashscope" => {
                    body["functions"] = tools.clone();
                }
                _ => {
                    let array = tools.as_array().cloned().unwrap_or_default();
                    let wrapped: Vec<Value> = array
                        .into_iter()
                        .map(|t| json!({ "type": "function", "function": t }))
                        .collect();
                    body["tools"] = Value::from(wrapped);
                }
            }
        }

        if let Some(choice) = context.get("tool_choice") {
            match dialect {
                "dashscope" => body["function_call"] = choice.clone(),
                _ => body["tool_choice"] = choice.clone(),
            }
        }

        if context
            .get("reasoning")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            match dialect {
                "dashscope" => body["enable_chain_of_thought"] = json!(true),
                _ => body["reasoning"] = json!({ "effort": "medium" }),
            }
        }

        let url = format!(
            "{}/v1/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let start = Instant::now();
        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send();
        let latency = start.elapsed().as_millis() as u64;

        match resp {
            Ok(r) => {
                let status_ok = r.status().is_success();
                let json: Value = r
                    .json()
                    .unwrap_or_else(|e| json!({ "error": e.to_string() }));
                let cost = json.get("usage").cloned().unwrap_or_else(|| json!({}));
                Reply {
                    ok: status_ok,
                    output: json,
                    latency_ms: latency,
                    cost,
                }
            }
            Err(e) => Reply {
                ok: false,
                output: json!({ "error": e.to_string() }),
                latency_ms: latency,
                cost: json!({}),
            },
        }
    }
}
