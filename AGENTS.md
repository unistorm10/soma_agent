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

## Phased Plan
1) Phase 0 — Spec Freeze & Parity Oracle
2) Phase 1 — Agent Core (no I/O)
3) Phase 2 — HTTP Backend
4) Phase 3 — MCP Client Integration
5) Phase 4 — Sandboxed Exec
6) Phase 5 — Local & Distributed Backends
7) Phase 6 — Telemetry & Habit Signals
