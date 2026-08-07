//! Integration tests: the real `ReadNodeClient` explore-deploy HTTP code against
//! the in-process fake observer ([`firefly_test_node`]). Loopback-only.
//!
//! The scripted empty→populated sequence models the observer visibility lag a
//! retrying caller must tolerate: the first reads return no value, then a later
//! read returns the finalized value.

use std::time::Duration;

use firefly_client::ReadNodeClient;
use firefly_client::errors::ReadNodeError;
use firefly_test_node::{FakeNode, explore};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_on_empty_then_populated() {
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
        .get_data_with_retry("theContract".to_owned(), 5, Duration::from_millis(10))
        .await
        .expect("value becomes visible after the lag clears");

    assert_eq!(value, "ready");
    // Two empty reads plus the populated one.
    assert_eq!(node.explore().calls().len(), 3);
    assert!(
        node.explore()
            .calls()
            .iter()
            .all(|term| term == "theContract"),
        "the client must forward the term on every attempt"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_data_success() {
    let node = FakeNode::builder()
        .explore_responses(vec![explore::string("hi")])
        .start()
        .await
        .expect("start fake node");

    let client = ReadNodeClient::new(node.observer_http_url());
    let value: String = client.get_data("q".to_owned()).await.expect("read value");
    assert_eq!(value, "hi");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_data_returns_value_missing_on_empty() {
    let node = FakeNode::builder()
        .explore_responses(vec![explore::empty()])
        .start()
        .await
        .expect("start fake node");

    let client = ReadNodeClient::new(node.observer_http_url());
    let result = client.get_data::<String>("q".to_owned()).await;
    assert!(
        matches!(result, Err(ReadNodeError::ReturnValueMissing)),
        "expected ReturnValueMissing, got {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_data_or_none_with_retry_is_none_when_never_populated() {
    let node = FakeNode::builder()
        .explore_responses(vec![explore::empty()])
        .start()
        .await
        .expect("start fake node");

    let client = ReadNodeClient::new(node.observer_http_url());
    let value: Option<String> = client
        .get_data_or_none_with_retry("q".to_owned(), 2, Duration::from_millis(10))
        .await
        .expect("empty result maps to Ok(None)");
    assert_eq!(value, None);
}
