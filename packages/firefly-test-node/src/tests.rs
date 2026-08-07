//! Self-tests: drive the real `firefly-client` transports against the fake to
//! prove each server surface end to end. The fuller behavioural matrix lives in
//! `firefly-client`'s integration tests; these guard the fake's own contract.

use std::time::Duration;

use firefly_client::models::{BlockId, DeployData, DeployId, WalletAddress};
use firefly_client::{NodeEvents, ReadNodeClient, WriteNodeClient};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::{ChannelValue, DeployOutcome, FakeNode, InjectedDeploy, explore};

/// Deterministic keypair from a seed byte, so tests never touch the RNG.
fn keypair(seed: u8) -> (SecretKey, PublicKey) {
    let mut bytes = [0u8; 32];
    bytes[31] = seed.max(1);
    let secret = SecretKey::from_byte_array(bytes).expect("valid secret key");
    let public = PublicKey::from_secret_key(&Secp256k1::new(), &secret);
    (secret, public)
}

#[tokio::test]
async fn addresses_are_loopback() {
    let node = FakeNode::start().await.expect("start fake node");
    assert!(node.grpc_addr().ip().is_loopback());
    assert!(node.observer_addr().ip().is_loopback());
    assert_ne!(node.grpc_addr().port(), 0);
    assert_ne!(node.observer_addr().port(), 0);
    assert_ne!(node.grpc_addr().port(), node.observer_addr().port());
}

#[tokio::test]
async fn status_endpoint_returns_ok() {
    let node = FakeNode::start().await.expect("start fake node");
    let response = reqwest::Client::new()
        .get(format!("{}/status", node.observer_http_url()))
        .send()
        .await
        .expect("status request");
    assert!(response.status().is_success());
    let body: serde_json::Value = response.json().await.expect("status json");
    assert_eq!(body["status"], "ok");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_accepts_and_reports_head_block() {
    let node = FakeNode::builder()
        .head_block_number(9)
        .deploy_outcome(DeployOutcome::Accepted("deploy-9".to_owned()))
        .start()
        .await
        .expect("start fake node");

    let (key, _) = keypair(1);
    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect write client");

    assert_eq!(client.get_head_block_index().await.expect("head block"), 9);

    let deploy_id = client
        .deploy(
            &key,
            DeployData::builder("new x in { Nil }".to_owned()).build(),
        )
        .await
        .expect("deploy accepted");
    assert_eq!(deploy_id, DeployId::from("deploy-9".to_owned()));

    let terms = node.deploy().recorded_deploy_terms();
    assert_eq!(terms, vec!["new x in { Nil }".to_owned()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channel_value_roundtrip() {
    let node = FakeNode::start().await.expect("start fake node");
    node.deploy()
        .set_channel_value("registry", ChannelValue::String("value-42".to_owned()));

    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect write client");

    let value: String = client
        .get_channel_value(BlockId::from("block-1".to_owned()), "registry".to_owned())
        .await
        .expect("channel read");
    assert_eq!(value, "value-42");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explore_retry_empty_then_populated() {
    let node = FakeNode::builder()
        .explore_responses(vec![
            explore::empty(),
            explore::empty(),
            explore::string("ready"),
        ])
        .start()
        .await
        .expect("start fake node");

    let client = ReadNodeClient::new(node.observer_http_url());
    let value: String = client
        .get_data_with_retry("query".to_owned(), 5, Duration::from_millis(10))
        .await
        .expect("read after visibility lag clears");
    assert_eq!(value, "ready");
    assert_eq!(node.explore().calls().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_started_and_wait_for_deploy_finalized() {
    let node = FakeNode::start().await.expect("start fake node");
    let (_, deployer) = keypair(2);

    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    let deploy_id = DeployId::from("ws-deploy".to_owned());
    let waiting = events.wait_for_deploy(&deploy_id, Duration::from_secs(5));

    node.events().inject_block_finalised(
        "block-1",
        vec![InjectedDeploy {
            id: "ws-deploy".to_owned(),
            cost: 100,
            deployer,
            errored: false,
        }],
    );

    assert_eq!(waiting.await, Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_reconnect_delivers_after_disconnect() {
    let node = FakeNode::start().await.expect("start fake node");
    let (_, deployer) = keypair(3);

    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_total_connections(1).await;

    // Drop the client's socket; a reconnect-capable client re-establishes it.
    node.events().disconnect_all();
    node.events().wait_for_total_connections(2).await;

    let deploy_id = DeployId::from("post-reconnect".to_owned());
    let waiting = events.wait_for_deploy(&deploy_id, Duration::from_secs(5));
    node.events().inject_block_finalised(
        "block-2",
        vec![InjectedDeploy {
            id: "post-reconnect".to_owned(),
            cost: 1,
            deployer,
            errored: false,
        }],
    );

    assert_eq!(waiting.await, Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_deployer_filtered_subscription() {
    use firefly_client::node_events::DeployEvent;
    use futures::StreamExt as _;

    let node = FakeNode::start().await.expect("start fake node");
    let (_, wanted) = keypair(4);
    let (_, other) = keypair(5);

    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    let mut subscription = events.subscribe_for_deploys(WalletAddress::from(wanted));

    // A deploy for a different wallet must not surface on this subscription.
    node.events().inject_block_finalised(
        "block-other",
        vec![InjectedDeploy {
            id: "other".to_owned(),
            cost: 1,
            deployer: other,
            errored: false,
        }],
    );
    // The wallet's own deploy must.
    node.events().inject_block_finalised(
        "block-wanted",
        vec![InjectedDeploy {
            id: "wanted".to_owned(),
            cost: 7,
            deployer: wanted,
            errored: false,
        }],
    );

    let event = tokio::time::timeout(Duration::from_secs(5), subscription.next())
        .await
        .expect("subscription yields within timeout")
        .expect("subscription is still open");
    let DeployEvent::Finalized { id, cost, errored } = event;
    assert_eq!(id, DeployId::from("wanted".to_owned()));
    assert_eq!(cost, 7);
    assert!(!errored);
}
