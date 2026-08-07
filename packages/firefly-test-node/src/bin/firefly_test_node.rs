//! Manual runner for the fake node.
//!
//! Starts a [`FakeNode`](firefly_test_node::FakeNode) on ephemeral loopback
//! ports, prints the three endpoints as JSON, and blocks until `Ctrl-C`. Useful
//! for pointing a client or a browser at the doubles by hand. Loopback-only.

use firefly_test_node::FakeNode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let node = FakeNode::start().await?;

    let endpoints = serde_json::json!({
        "validator_grpc": node.grpc_endpoint(),
        "observer_http": node.observer_http_url(),
        "observer_ws": node.observer_ws_url(),
    });
    println!("{endpoints}");
    println!("firefly-test-node listening on loopback; press Ctrl-C to stop");

    tokio::signal::ctrl_c().await?;
    node.shutdown().await;
    Ok(())
}
