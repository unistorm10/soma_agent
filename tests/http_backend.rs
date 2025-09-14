use std::time::Duration;

use httpmock::prelude::*;
use serde_json::json;

use soma_agent::{
    backends::http::{HttpConfig, HttpProvider},
    Ask, Provider,
};

#[test]
fn openai_dialect_maps_fields() {
    let server = MockServer::start();

    let expected = json!({
        "model": "gpt-test",
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [{
            "type": "function",
            "function": {"name": "ping", "description": "", "parameters": {}}
        }],
        "tool_choice": "auto",
        "reasoning": {"effort": "medium"}
    });

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .json_body(expected.clone());
        then.status(200)
            .json_body(json!({"id": "1", "usage": {"total_tokens": 1}}));
    });

    let config = HttpConfig {
        base_url: server.base_url(),
        model: "gpt-test".into(),
        api_key: "k".into(),
        timeout: Duration::from_secs(1),
    };
    let provider = HttpProvider::new(config);

    let ask = Ask {
        op: "chat".into(),
        input: json!([{ "role": "user", "content": "hi" }]),
        context: json!({
            "tools": [{ "name": "ping", "description": "", "parameters": {} }],
            "tool_choice": "auto",
            "reasoning": true
        }),
    };

    let reply = provider.ask(ask);
    mock.assert();
    assert!(reply.ok);
    assert_eq!(reply.output["id"], "1");
}

#[test]
fn dashscope_dialect_maps_fields() {
    let server = MockServer::start();

    let expected = json!({
        "model": "qwen-test",
        "messages": [{"role": "user", "content": "hi"}],
        "functions": [{ "name": "ping", "description": "", "parameters": {} }],
        "function_call": "none",
        "enable_chain_of_thought": true
    });

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .json_body(expected.clone());
        then.status(200).json_body(json!({"id": "2"}));
    });

    let config = HttpConfig {
        base_url: server.base_url(),
        model: "qwen-test".into(),
        api_key: "k".into(),
        timeout: Duration::from_secs(1),
    };
    let provider = HttpProvider::new(config);

    let ask = Ask {
        op: "chat".into(),
        input: json!([{ "role": "user", "content": "hi" }]),
        context: json!({
            "dialect": "dashscope",
            "tools": [{ "name": "ping", "description": "", "parameters": {} }],
            "tool_choice": "none",
            "reasoning": true
        }),
    };

    let reply = provider.ask(ask);
    mock.assert();
    assert!(reply.ok);
    assert_eq!(reply.output["id"], "2");
}
