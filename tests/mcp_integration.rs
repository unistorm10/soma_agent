use httpmock::prelude::*;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use soma_agent::{Agent, Ask, Provider, ProviderKind, Reply, ToolSpec};

struct Dummy;

impl Provider for Dummy {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Embedded
    }

    fn ask(&self, _ask: Ask) -> Reply {
        Reply {
            ok: true,
            output: json!({}),
            latency_ms: 0,
            cost: json!({}),
        }
    }
}

#[test]
fn mcp_tool_from_config_invokes() {
    let server = MockServer::start();
    let _handshake = server.mock(|when, then| {
        when.method(POST)
            .json_body_partial(json!({"method": "handshake"}).to_string());
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}}));
    });
    let _schema = server.mock(|when, then| {
        when.method(POST)
            .json_body_partial(json!({"method": "schema"}).to_string());
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":2,"result":{}}));
    });
    let _invoke = server.mock(|when, then| {
        when.method(POST)
            .json_body_partial(json!({"method": "invoke"}).to_string());
        then.status(200)
            .json_body(json!({"jsonrpc":"2.0","id":3,"result":"pong"}));
    });

    let cfg_path = std::env::temp_dir().join("mcp_cfg.json");
    std::fs::write(&cfg_path, format!("{{\"ping\": \"{}\"}}", server.url("/"))).unwrap();

    let mut agent = Agent::new(Dummy, 1, 1000, 1, CancellationToken::new());
    agent
        .register_tool("cfg", ToolSpec::McpConfigFile(cfg_path))
        .unwrap();

    assert!(agent.has_tool("ping"));
    let reply = agent
        .call_tool(
            "ping",
            Ask {
                op: "ping".into(),
                input: json!({}),
                context: json!({}),
            },
        )
        .unwrap();
    assert_eq!(reply.output, json!("pong"));

    _handshake.assert();
    _schema.assert();
    _invoke.assert();
}
