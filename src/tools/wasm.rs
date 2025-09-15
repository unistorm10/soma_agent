use crate::{Ask, Provider, ProviderKind, Reply};
use serde_json::json;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder};
use wasmtime_wasi::{preview1::add_to_linker_sync, preview1::WasiP1Ctx, WasiCtxBuilder};

/// WasmTool executes WebAssembly modules inside a sandbox using wasmtime.
pub struct WasmTool {
    engine: Engine,
    module: Module,
    fuel: u64,
    memory_limit: Option<usize>,
    timeout: Duration,
}

impl WasmTool {
    /// Create a new WasmTool from raw WebAssembly bytes.
    pub fn from_bytes(
        wasm: &[u8],
        fuel: u64,
        memory_limit: Option<usize>,
        timeout: Duration,
    ) -> Result<Self, wasmtime::Error> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        let module = Module::from_binary(&engine, wasm)?;
        Ok(Self {
            engine,
            module,
            fuel,
            memory_limit,
            timeout,
        })
    }
}

impl Provider for WasmTool {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Embedded
    }

    fn ask(&self, ask: Ask) -> Reply {
        let start = Instant::now();
        let func = ask.op.clone();
        let arg = ask.input.as_i64().unwrap_or(0) as i32;
        let engine = self.engine.clone();
        let module = self.module.clone();
        let fuel = self.fuel;
        let mem = self.memory_limit;
        let timeout = self.timeout;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let wasi = WasiCtxBuilder::new().build_p1();
            let limits_builder = if let Some(limit) = mem {
                StoreLimitsBuilder::new().memory_size(limit).instances(1)
            } else {
                StoreLimitsBuilder::new()
            };
            struct Ctx {
                wasi: WasiP1Ctx,
                limits: wasmtime::StoreLimits,
            }
            let ctx = Ctx {
                wasi,
                limits: limits_builder.build(),
            };
            let mut store: Store<Ctx> = Store::new(&engine, ctx);
            store.limiter(|cx| &mut cx.limits);
            store.set_fuel(fuel).ok();
            let mut linker: Linker<Ctx> = Linker::new(&engine);
            add_to_linker_sync(&mut linker, |cx| &mut cx.wasi).unwrap();
            let instance = match linker.instantiate(&mut store, &module) {
                Ok(i) => i,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };
            let func = match instance.get_typed_func::<i32, i32>(&mut store, &func) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };
            let result = func.call(&mut store, arg).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(val)) => Reply {
                ok: true,
                output: json!(val),
                latency_ms: start.elapsed().as_millis() as u64,
                cost: json!({}),
            },
            Ok(Err(err)) => Reply {
                ok: false,
                output: json!({ "error": err }),
                latency_ms: start.elapsed().as_millis() as u64,
                cost: json!({}),
            },
            Err(_) => Reply {
                ok: false,
                output: json!({ "error": "timeout" }),
                latency_ms: timeout.as_millis() as u64,
                cost: json!({}),
            },
        }
    }
}
