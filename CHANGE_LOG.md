# CHANGE LOG

- 2025-09-11 — OpenAI ChatGPT — initial crate scaffolding and universal module interface; affected: Cargo.toml, src/lib.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-11 — OpenAI ChatGPT — add initial Qwen-Agent parity fixture and validation test; affected: fixtures/function_calling_weather.json, tests/parity_fixture.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-11 — OpenAI ChatGPT — add multi-step parity fixture and test; affected: fixtures/multi_step_tool_calls.json, tests/parity_fixture.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-12 — OpenAI ChatGPT — add parallel tool-call parity fixture and test; affected: fixtures/parallel_tool_calls.json, tests/parity_fixture.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-12 — OpenAI ChatGPT — document reasoning trace capture attempt; affected: PROGRESS.md, PENDING_TASKS.md, RUN_REPORT.md, TEST_REPORT.md
- 2025-09-12 — OpenAI ChatGPT — add step-limited agent loop and tests; affected: src/lib.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-13 — OpenAI ChatGPT — add reasoning policy heuristics to agent core; affected: src/lib.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-13 — OpenAI ChatGPT — add token budget guardrails to agent core; affected: src/lib.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md, RUN_REPORT.md, TEST_REPORT.md
- 2025-09-14 — OpenAI ChatGPT — implement tool routing in agent core and test; affected: src/lib.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md
- 2025-09-15 — OpenAI ChatGPT — add synthetic reasoning trace fixture and test; affected: fixtures/reasoning_trace.json, tests/parity_fixture.rs, AGENT.md, PROGRESS.md, PENDING_TASKS.md, RUN_REPORT.md, TEST_REPORT.md
- 2025-09-12 — OpenAI ChatGPT — note missing DashScope API key blocking reasoning trace capture; affected: PROGRESS.md, PENDING_TASKS.md, RUN_REPORT.md, TEST_REPORT.md
- 2025-09-12 — OpenAI ChatGPT — document repeated reasoning trace capture attempt; affected: PROGRESS.md, RUN_REPORT.md, TEST_REPORT.md
- 2025-09-13 — OpenAI ChatGPT — rename agent guide and add custom image generation fixture; affected: AGENTS.md, fixtures/custom_image_tool.json, tests/parity_fixture.rs, PROGRESS.md, PENDING_TASKS.md
- 2025-09-13 — OpenAI ChatGPT — note provider API in guide and record DashScope trace attempt; affected: AGENTS.md, PROGRESS.md, RUN_REPORT.md, TEST_REPORT.md

- 2025-09-13 — OpenAI ChatGPT — document another failed reasoning trace capture attempt; affected: PROGRESS.md, RUN_REPORT.md, TEST_REPORT.md
