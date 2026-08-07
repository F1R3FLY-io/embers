//! Integration tests: the real `WriteNodeClient` gRPC code against the
//! in-process fake validator ([`firefly_test_node`]). Loopback-only.

use firefly_client::WriteNodeClient;
use firefly_client::models::{BlockId, DeployData, DeployId};
use firefly_test_node::{ChannelValue, DeployOutcome, FakeNode};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

fn keypair(seed: u8) -> (SecretKey, PublicKey) {
    let mut bytes = [0u8; 32];
    bytes[31] = seed.max(1);
    let secret = SecretKey::from_byte_array(bytes).expect("valid secret key");
    let public = PublicKey::from_secret_key(&Secp256k1::new(), &secret);
    (secret, public)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_happy_path_returns_id() {
    let node = FakeNode::builder()
        .head_block_number(3)
        .deploy_outcome(DeployOutcome::Accepted("abc123".to_owned()))
        .start()
        .await
        .expect("start fake node");

    let (key, _) = keypair(1);
    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect");

    let deploy_id = client
        .deploy(
            &key,
            DeployData::builder("new x in { Nil }".to_owned()).build(),
        )
        .await
        .expect("deploy accepted");

    assert_eq!(deploy_id, DeployId::from("abc123".to_owned()));
    // The deploy path first reads the head block, then submits the term.
    assert_eq!(
        node.deploy().recorded_deploy_terms(),
        vec!["new x in { Nil }".to_owned()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_deploy_returns_id() {
    let node = FakeNode::builder()
        .deploy_outcome(DeployOutcome::Accepted("full-1".to_owned()))
        .start()
        .await
        .expect("start fake node");

    let (key, _) = keypair(1);
    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect");

    let deploy_id = client
        .full_deploy(&key, DeployData::builder("Nil".to_owned()).build())
        .await
        .expect("full deploy accepted");
    assert_eq!(deploy_id, DeployId::from("full-1".to_owned()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploy_error_is_surfaced() {
    let node = FakeNode::builder()
        .deploy_outcome(DeployOutcome::Error(vec!["insufficient funds".to_owned()]))
        .start()
        .await
        .expect("start fake node");

    let (key, _) = keypair(1);
    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect");

    let error = client
        .deploy(&key, DeployData::builder("Nil".to_owned()).build())
        .await
        .expect_err("node rejected the deploy");

    let message = error.to_string();
    assert!(
        message.contains("do_deploy error"),
        "expected a do_deploy error, got: {message}"
    );
    assert!(
        message.contains("insufficient funds"),
        "expected the node's message to be surfaced, got: {message}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn head_block_index_read() {
    let node = FakeNode::builder()
        .head_block_number(4242)
        .start()
        .await
        .expect("start fake node");

    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect");

    assert_eq!(
        client.get_head_block_index().await.expect("head block"),
        4242
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channel_value_read() {
    let node = FakeNode::start().await.expect("start fake node");
    node.deploy()
        .set_channel_value("greeting", ChannelValue::String("hello world".to_owned()));

    let mut client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .expect("connect");

    let value: String = client
        .get_channel_value(BlockId::from("block-1".to_owned()), "greeting".to_owned())
        .await
        .expect("channel read");
    assert_eq!(value, "hello world");
}
