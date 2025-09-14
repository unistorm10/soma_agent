use serde_json::json;
use serde_json::Value;
use soma_agent::{Agent, Ask, Provider, ProviderKind, Reply};
use std::cell::Cell;

#[test]
fn function_calling_weather_fixture_valid() {
    let data: Value =
        serde_json::from_str(include_str!("../fixtures/function_calling_weather.json"))
            .expect("valid JSON");

    // validate presence of expected fields
    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_call"];
    assert_eq!(expected["name"], "get_current_weather");
    assert_eq!(expected["arguments"]["location"], "San Francisco");
    assert_eq!(expected["arguments"]["unit"], "fahrenheit");
}

#[test]
fn multi_step_tool_calls_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/multi_step_tool_calls.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_calls"];
    assert!(expected.is_array());
    assert_eq!(expected.as_array().unwrap().len(), 2);

    let first = &expected[0];
    assert_eq!(first["name"], "get_current_weather");
    assert_eq!(first["arguments"]["location"], "San Francisco");
    assert_eq!(first["arguments"]["unit"], "fahrenheit");

    let second = &expected[1];
    assert_eq!(second["name"], "get_weather_forecast");
    assert_eq!(second["arguments"]["location"], "San Francisco");
    assert_eq!(second["arguments"]["days"], 2);
}

#[test]
fn parallel_tool_calls_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/parallel_tool_calls.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_calls"];
    assert!(expected.is_array());
    assert_eq!(expected.as_array().unwrap().len(), 3);

    let sf = &expected[0];
    assert_eq!(sf["name"], "get_current_weather");
    assert_eq!(sf["arguments"]["location"], "San Francisco");
    assert_eq!(sf["arguments"]["unit"], "fahrenheit");

    let tokyo = &expected[1];
    assert_eq!(tokyo["name"], "get_current_weather");
    assert_eq!(tokyo["arguments"]["location"], "Tokyo");
    assert_eq!(tokyo["arguments"]["unit"], "celsius");

    let paris = &expected[2];
    assert_eq!(paris["name"], "get_current_weather");
    assert_eq!(paris["arguments"]["location"], "Paris");
    assert_eq!(paris["arguments"]["unit"], "celsius");
}

#[test]
fn reasoning_trace_fixture_valid() {
    let data: Value =
        serde_json::from_str(include_str!("../fixtures/reasoning_trace.json")).expect("valid JSON");

    assert!(data["messages"].is_array());
    let response = &data["response"];
    assert_eq!(response["content"], "4");
    assert_eq!(
        response["reasoning_content"],
        "To compute 2 + 2, add the numbers to get 4."
    );
}
#[test]
fn custom_image_tool_fixture_valid() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/custom_image_tool.json"))
        .expect("valid JSON");

    assert!(data["messages"].is_array());
    assert!(data["functions"].is_array());

    let expected = &data["expected_tool_call"];
    assert_eq!(expected["name"], "my_image_gen");
    assert_eq!(expected["arguments"]["prompt"], "A cute dachshund");
}

struct Adder;

impl Provider for Adder {
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

struct SingleToolProvider {
    seen: Cell<bool>,
}

impl Provider for SingleToolProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Embedded
    }

    fn ask(&self, ask: Ask) -> Reply {
        if self.seen.get() {
            Reply {
                ok: true,
                output: json!({"final": ask.input["sum"].clone()}),
                latency_ms: 0,
                cost: json!({}),
            }
        } else {
            self.seen.set(true);
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

struct ParallelToolProvider {
    seen: Cell<bool>,
}

impl Provider for ParallelToolProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Embedded
    }

    fn ask(&self, ask: Ask) -> Reply {
        if self.seen.get() {
            let arr = ask.input.as_array().unwrap();
            let first = arr[0]["sum"].as_i64().unwrap();
            let second = arr[1]["sum"].as_i64().unwrap();
            Reply {
                ok: true,
                output: json!({"sums": [first, second]}),
                latency_ms: 0,
                cost: json!({}),
            }
        } else {
            self.seen.set(true);
            Reply {
                ok: false,
                output: json!({
                    "tool_calls": [
                        {"op": "adder", "input": {"a": 1, "b": 2}},
                        {"op": "adder", "input": {"a": 3, "b": 4}}
                    ]
                }),
                latency_ms: 0,
                cost: json!({}),
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn agent_sequential_tool_call() {
    let ask = Ask {
        op: "sum".into(),
        input: json!({}),
        context: json!({}),
    };
    let mut agent = Agent::new(
        SingleToolProvider {
            seen: Cell::new(false),
        },
        3,
        1000,
    );
    agent.register_tool("adder", Adder);
    let reply = agent.run(ask).await;
    assert!(reply.ok);
    assert_eq!(reply.output, json!({"final": 3}));
}

#[tokio::test(flavor = "current_thread")]
async fn agent_parallel_tool_calls() {
    let ask = Ask {
        op: "sum".into(),
        input: json!({}),
        context: json!({}),
    };
    let mut agent = Agent::new(
        ParallelToolProvider {
            seen: Cell::new(false),
        },
        3,
        1000,
    );
    agent.register_tool("adder", Adder);
    let reply = agent.run(ask).await;
    assert!(reply.ok);
    assert_eq!(reply.output, json!({"sums": [3, 7]}));
}
