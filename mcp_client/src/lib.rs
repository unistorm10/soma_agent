use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("rpc error: {0}")]
    Rpc(Value),
}

pub struct McpClient {
    base_url: String,
    http: Client,
    id: AtomicU64,
}

impl McpClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, Error> {
        let client = Client::new();
        let this = Self {
            base_url: base_url.into(),
            http: client,
            id: AtomicU64::new(1),
        };
        // Perform handshake to ensure server is reachable
        let _ = this.handshake()?;
        Ok(this)
    }

    fn rpc(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.id.fetch_add(1, Ordering::SeqCst);
        let req = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let resp: Value = self.http.post(&self.base_url).json(&req).send()?.json()?;
        if let Some(err) = resp.get("error") {
            return Err(Error::Rpc(err.clone()));
        }
        Ok(resp["result"].clone())
    }

    pub fn handshake(&self) -> Result<Value, Error> {
        self.rpc("handshake", json!({}))
    }

    pub fn schema(&self, tool: &str) -> Result<Value, Error> {
        self.rpc("schema", json!({"tool": tool}))
    }

    pub fn invoke(&self, tool: &str, input: Value) -> Result<Value, Error> {
        self.rpc("invoke", json!({"tool": tool, "input": input}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[test]
    fn client_invokes() {
        let server = MockServer::start();
        let _handshake = server.mock(|when, then| {
            when.method(POST)
                .json_partial(json!({"method":"handshake"}));
            then.status(200)
                .json_body(json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}}));
        });
        let _invoke = server.mock(|when, then| {
            when.method(POST).json_partial(json!({"method":"invoke"}));
            then.status(200)
                .json_body(json!({"jsonrpc":"2.0","id":2,"result":{"pong":true}}));
        });

        let client = McpClient::new(server.url("/")).unwrap();
        let out = client.invoke("ping", json!({})).unwrap();
        assert_eq!(out, json!({"pong":true}));
    }
}
