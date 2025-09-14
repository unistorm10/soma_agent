use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde_json::{json, Value};

use crate::{Ask, Provider, ProviderKind, Reply};
use mcp_client::{Error as McpError, McpClient};

pub struct McpProvider {
    client: McpClient,
    schemas: Mutex<HashMap<String, Value>>,
}

impl McpProvider {
    pub fn new(url: impl Into<String>) -> Result<Self, McpError> {
        let client = McpClient::new(url.into())?;
        Ok(Self {
            client,
            schemas: Mutex::new(HashMap::new()),
        })
    }
}

impl Provider for McpProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::RemoteGrpc
    }

    fn ask(&self, ask: Ask) -> Reply {
        let start = Instant::now();
        {
            let mut schemas = self.schemas.lock().unwrap();
            if !schemas.contains_key(&ask.op) {
                if let Ok(schema) = self.client.schema(&ask.op) {
                    schemas.insert(ask.op.clone(), schema);
                }
            }
        }
        match self.client.invoke(&ask.op, ask.input.clone()) {
            Ok(out) => Reply {
                ok: true,
                output: out,
                latency_ms: start.elapsed().as_millis() as u64,
                cost: json!({}),
            },
            Err(e) => Reply {
                ok: false,
                output: json!({"error": e.to_string()}),
                latency_ms: start.elapsed().as_millis() as u64,
                cost: json!({}),
            },
        }
    }
}
