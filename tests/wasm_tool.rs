#![cfg(feature = "sandboxed_exec")]

use serde_json::json;
use soma_agent::{tools::WasmTool, Ask, Provider};
use std::time::Duration;

#[test]
fn executes_simple_wasm() {
    let wat = r#"(module (func (export "double") (param i32) (result i32)
                        local.get 0 i32.const 2 i32.mul))"#;
    let wasm = wat::parse_str(wat).unwrap();
    let tool = WasmTool::from_bytes(&wasm, 10_000, None, Duration::from_secs(1)).unwrap();
    let ask = Ask {
        op: "double".into(),
        input: json!(21),
        context: json!({}),
    };
    let reply = tool.ask(ask);
    assert!(reply.ok);
    assert_eq!(reply.output, json!(42));
}

#[test]
fn enforces_cpu_limit() {
    let wat = r#"(module (func (export "burn") (param i32) (result i32) (loop br 0) i32.const 0))"#;
    let wasm = wat::parse_str(wat).unwrap();
    let tool = WasmTool::from_bytes(&wasm, 100, None, Duration::from_secs(1)).unwrap();
    let ask = Ask {
        op: "burn".into(),
        input: json!(0),
        context: json!({}),
    };
    let reply = tool.ask(ask);
    assert!(!reply.ok);
}

#[test]
fn enforces_time_limit() {
    let wat = r#"(module (func (export "burn") (param i32) (result i32) (loop br 0) i32.const 0))"#;
    let wasm = wat::parse_str(wat).unwrap();
    let tool = WasmTool::from_bytes(&wasm, u64::MAX, None, Duration::from_millis(50)).unwrap();
    let ask = Ask {
        op: "burn".into(),
        input: json!(0),
        context: json!({}),
    };
    let reply = tool.ask(ask);
    assert_eq!(reply.output, json!({"error": "timeout"}));
}

#[test]
fn enforces_memory_limit() {
    let wat = r#"(module
        (memory 1)
        (func (export "grow") (result i32)
            i32.const 1
            memory.grow
            drop
            i32.const 0))"#;
    let wasm = wat::parse_str(wat).unwrap();
    let tool = WasmTool::from_bytes(&wasm, 10_000, Some(65_536), Duration::from_secs(1)).unwrap();
    let ask = Ask {
        op: "grow".into(),
        input: json!(0),
        context: json!({}),
    };
    let reply = tool.ask(ask);
    assert!(!reply.ok);
}
