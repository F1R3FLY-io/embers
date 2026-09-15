//! Integration tests: the real `NodeEvents` WebSocket client against the
//! in-process fake observer ([`firefly_test_node`]). Loopback-only.
//!
//! `wait_for_deploy` is three-valued: `Some(false)` finalized cleanly,
//! `Some(true)` finalized but the deploy errored, `None` timed out.

use std::time::Duration;

use firefly_client::NodeEvents;
use firefly_client::models::{DeployId, WalletAddress};
use firefly_client::node_events::DeployEvent;
use firefly_test_node::{FakeNode, InjectedDeploy};
use futures::StreamExt as _;
use secp256k1::{PublicKey, Secp256k1, SecretKey};

fn public_key(seed: u8) -> PublicKey {
    let mut bytes = [0u8; 32];
    bytes[31] = seed.max(1);
    let secret = SecretKey::from_byte_array(bytes).expect("valid secret key");
    PublicKey::from_secret_key(&Secp256k1::new(), &secret)
}

fn deploy(id: &str, deployer: PublicKey, errored: bool) -> InjectedDeploy {
    InjectedDeploy {
        id: id.to_owned(),
        cost: 1,
        deployer,
        errored,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_for_deploy_clean_finalize() {
    let node = FakeNode::start().await.expect("start fake node");
    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    let deploy_id = DeployId::from("clean".to_owned());
    let waiting = events.wait_for_deploy(&deploy_id, Duration::from_secs(5));
    node.events()
        .inject_block_finalised("block-1", vec![deploy("clean", public_key(1), false)]);

    assert_eq!(waiting.await, Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_for_deploy_errored_deploy() {
    let node = FakeNode::start().await.expect("start fake node");
    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    let deploy_id = DeployId::from("boom".to_owned());
    let waiting = events.wait_for_deploy(&deploy_id, Duration::from_secs(5));
    node.events()
        .inject_block_finalised("block-1", vec![deploy("boom", public_key(1), true)]);

    assert_eq!(waiting.await, Some(true));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_for_deploy_times_out() {
    let node = FakeNode::start().await.expect("start fake node");
    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    // Nothing is ever injected for this id, so the wait must time out.
    let deploy_id = DeployId::from("never".to_owned());
    let result = events
        .wait_for_deploy(&deploy_id, Duration::from_millis(150))
        .await;
    assert_eq!(result, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deployer_filtered_subscription() {
    let node = FakeNode::start().await.expect("start fake node");
    let wanted = public_key(2);
    let other = public_key(3);

    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_connections(1).await;

    let mut subscription = events.subscribe_for_deploys(WalletAddress::from(wanted));

    // Another wallet's deploy first, then ours: only ours may surface.
    node.events()
        .inject_block_finalised("block-other", vec![deploy("other", other, false)]);
    node.events()
        .inject_block_finalised("block-mine", vec![deploy("mine", wanted, false)]);

    let event = tokio::time::timeout(Duration::from_secs(5), subscription.next())
        .await
        .expect("subscription yields within timeout")
        .expect("subscription is still open");
    let DeployEvent::Finalized { id, errored, .. } = event;
    assert_eq!(id, DeployId::from("mine".to_owned()));
    assert!(!errored);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_delivers_after_socket_drop() {
    let node = FakeNode::start().await.expect("start fake node");
    let events = NodeEvents::new(&node.observer_ws_url());
    node.events().wait_for_total_connections(1).await;

    // Force the server side to drop the socket; the client must reconnect.
    node.events().disconnect_all();
    node.events().wait_for_total_connections(2).await;

    let deploy_id = DeployId::from("after-reconnect".to_owned());
    let waiting = events.wait_for_deploy(&deploy_id, Duration::from_secs(5));
    node.events().inject_block_finalised(
        "block-2",
        vec![deploy("after-reconnect", public_key(4), false)],
    );

    assert_eq!(waiting.await, Some(false));
}
