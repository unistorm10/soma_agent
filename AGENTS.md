# AGENT — Soma Agent Core

## Mission
Provide a portable Rust agent runtime mirroring Qwen-Agent behaviors.

## Hard Requirements
- Rust-only implementation; no Python dependencies.
- Universal module pattern interface for providers.
- Tests and lint must pass before merge.
- No GitHub Actions workflows.

## SLOs / Acceptance
- All unit tests pass.
- `cargo fmt` and `cargo clippy` are clean.
- Parity fixtures reproduce Qwen-Agent behaviors (future).

## Invariants
- Providers communicate using `Ask` and `Reply` structs.
- Provider deployment form is captured by `ProviderKind`.

## Interfaces Changed
- Introduced universal provider API with `Ask`, `Reply`, `ProviderKind`, and a `Provider` trait.
- Added parity fixtures under `fixtures/` including single, parallel, and multi-step tool calls.
- Added synthetic reasoning trace fixture demonstrating the `reasoning_content` field.
- Added custom image generation fixture demonstrating non-weather tool usage.
- Introduced `Agent` stepper enforcing a step limit over provider calls.
- Embedded `ReasoningPolicy` heuristics selecting direct vs reasoned mode and injecting mode into request context.
- Added token budget guardrails that cap total tokens and downgrade reasoning near the limit.
- Introduced tool routing allowing the agent to execute tool calls via registered providers.
 - Agent now processes `tool_calls` arrays, running tools in parallel via `tokio::try_join!` and feeding aggregated results back to the provider.

- Agent now supports configurable retry limits and cancellation tokens with exponential backoff for provider and tool calls.
- Added HTTP backend provider using `HttpConfig { base_url, model, api_key, timeout }` that maps universal tool schemas,
  `tool_choice`, and reasoning flags to provider-specific fields (`tools`/`functions`, `tool_choice`/`function_call`,
  `reasoning`/`enable_chain_of_thought`).
- Added `mcp_client` crate and `McpProvider` for JSON-RPC servers.
- `Agent::register_tool` now accepts MCP endpoints or JSON config files via `ToolSpec`.

## HTTP Backend Usage
```rust
use soma_agent::{Ask, backends::http::{HttpConfig, HttpProvider}};
use serde_json::json;
use std::time::Duration;

let cfg = HttpConfig {
    base_url: "https://api.openai.com".into(),
    model: "gpt-4o".into(),
    api_key: std::env::var("OPENAI_API_KEY").unwrap(),
    timeout: Duration::from_secs(30),
};
let provider = HttpProvider::new(cfg);
let ask = Ask {
    op: "chat".into(),
    input: json!([{"role": "user", "content": "hi"}]),
    context: json!({
        "tools": [{"name": "ping", "description": "", "parameters": {}}],
        "tool_choice": "auto",
        "reasoning": true
    }),
};
let reply = provider.ask(ask);
```
Set `dialect` to `"dashscope"` in the context to emit DashScope field names
(`functions`, `function_call`, `enable_chain_of_thought`).

## MCP Server Configuration

Create a JSON file mapping tool names to MCP server URLs:

```json
{ "ping": "http://localhost:8080/" }
```

Register tools from the file:

```rust
use soma_agent::{Agent, ToolSpec};
use tokio_util::sync::CancellationToken;

let mut agent = Agent::new(provider, 1, 1000, 1, CancellationToken::new());
agent.register_tool("cfg", ToolSpec::McpConfigFile("tools.json".into())).unwrap();
```

## Phased Plan
1) Phase 0 — Spec Freeze & Parity Oracle
2) Phase 1 — Agent Core (no I/O)
3) Phase 2 — HTTP Backend
4) Phase 3 — MCP Client Integration
5) Phase 4 — Sandboxed Exec
6) Phase 5 — Local & Distributed Backends
7) Phase 6 — Telemetry & Habit Signals
