//! Manual runner for the loopback OpenAI stub.
//!
//! Binds the address in `OPENAI_STUB_ADDR` (default `127.0.0.1:8571`), prints the
//! endpoints as JSON, and blocks until `Ctrl-C`. Useful for pointing a node or a
//! `curl` at the stub by hand. The address must be loopback.

use std::net::SocketAddr;

use anyhow::Context as _;
use openai_stub::OpenAiStub;

/// Default address when `OPENAI_STUB_ADDR` is unset.
const DEFAULT_ADDR: &str = "127.0.0.1:8571";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr_str = std::env::var("OPENAI_STUB_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    let addr: SocketAddr = addr_str
        .parse()
        .with_context(|| format!("OPENAI_STUB_ADDR is not a valid socket address: {addr_str}"))?;

    let stub = OpenAiStub::start_on(addr).await?;

    let endpoints = serde_json::json!({
        "addr": stub.addr().to_string(),
        "origin": stub.origin(),
        "base_url": stub.base_url(),
    });
    println!("{endpoints}");
    println!("openai-stub listening on loopback; press Ctrl-C to stop");

    tokio::signal::ctrl_c().await?;
    stub.shutdown().await;
    Ok(())
}
