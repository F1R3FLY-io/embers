//! In-process HTTP tests: the real poem-openapi app served over
//! [`poem::test::TestClient`], with every domain service wired to the mock
//! node-client triple so requests exercise routing, extractors, the JWT
//! two-phase binding, and response mapping hermetically.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use firefly_client::helpers::sign_prepared_contract;
use poem::Endpoint;
use poem::http::StatusCode;
use poem::test::TestClient;
use poem_openapi::types::{Base64, ParseFromJSON, ToJSON};
use tempfile::TempDir;

use crate::app::{AppServices, build_app};
use crate::domain::agents::AgentsService;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::continuation_store::F1r3driveContinuationStore;
use crate::domain::oslfs::OslfsService;
use crate::domain::test_helpers::{
    MockNodeEventSource,
    MockReadNode,
    MockWriteNode,
    test_secret_key,
    test_uri,
};
use crate::domain::testnet::TestnetService;
use crate::domain::wallets::WalletsService;

mod jwt_security;
mod routing;
mod two_phase;

/// Fixed per-app JWT secret; tests mint matching tokens where needed.
const TEST_JWT_SECRET: &[u8] = b"embers-in-process-test-secret";

/// Handles onto the mock backends behind the in-process app.
struct Harness {
    /// The write node shared by all five services; records every deployed
    /// signed contract.
    write_node: MockWriteNode,
    /// Keeps the continuation-store directory alive as long as the app.
    _store_dir: TempDir,
}

/// Builds the real HTTP app over the mock node-client triple and returns a
/// test client driving it plus the mock handles for assertions.
fn test_app() -> (TestClient<impl Endpoint>, Harness) {
    let write_node = MockWriteNode::new();
    let read_node = MockReadNode::new();
    let node_events = MockNodeEventSource::new();

    let store_dir = tempfile::tempdir().expect("create a continuation store directory");
    let continuation_store =
        F1r3driveContinuationStore::open_for_test(store_dir.path(), [7u8; 32], [9u8; 32])
            .expect("open the test continuation store");

    let services = AppServices {
        agents: AgentsService {
            uri: test_uri(),
            write_client: write_node.clone(),
            read_client: read_node.clone(),
            observer_node_events: node_events.clone(),
        },
        agents_teams: AgentsTeamsService {
            uri: test_uri(),
            write_client: write_node.clone(),
            read_client: read_node.clone(),
            observer_node_events: node_events.clone(),
            service_key: test_secret_key(),
            aes_encryption_key: [42u8; 32].into(),
            continuation_store,
            deploy_finalization_timeout: Duration::from_secs(5),
            run_locks: Arc::new(DashMap::new()),
            firesky_accounts: Arc::new(DashMap::new()),
        },
        oslfs: OslfsService {
            uri: test_uri(),
            write_client: write_node.clone(),
            read_client: read_node.clone(),
            observer_node_events: node_events.clone(),
        },
        wallets: WalletsService {
            uri: test_uri(),
            write_client: write_node.clone(),
            read_client: read_node.clone(),
            validator_node_events: node_events.clone(),
            observer_node_events: node_events.clone(),
        },
        testnet: TestnetService {
            uri: test_uri(),
            service_key: test_secret_key(),
            write_client: write_node.clone(),
            read_client: read_node,
            observer_node_events: node_events,
            deploy_finalization_timeout: Duration::from_secs(5),
        },
    };

    let app = build_app(
        services,
        jsonwebtoken::EncodingKey::from_secret(TEST_JWT_SECRET),
        jsonwebtoken::DecodingKey::from_secret(TEST_JWT_SECRET),
    );

    (
        TestClient::new(app),
        Harness {
            write_node,
            _store_dir: store_dir,
        },
    )
}

/// Runs a prepare call and returns the parsed `PrepareResponse` JSON
/// (`{ "response": ..., "token": ... }`).
async fn prepare<E: Endpoint>(
    client: &TestClient<E>,
    path: &str,
    request: &serde_json::Value,
) -> serde_json::Value {
    let response = client.post(path).body_json(request).send().await;
    response.assert_status(StatusCode::OK);
    response.json().await.value().deserialize()
}

/// Signs the prepared contract from a prepare exchange with the test wallet
/// key and assembles the matching send body, echoing the prepare request and
/// response byte-identically. Returns the body plus the exact contract bytes
/// that were signed.
fn signed_send_body(
    prepare_request: &serde_json::Value,
    prepare_response: &serde_json::Value,
) -> (serde_json::Value, Vec<u8>) {
    let contract =
        Base64::<Vec<u8>>::parse_from_json(Some(prepare_response["response"]["contract"].clone()))
            .expect("the prepared contract must be base64")
            .0;

    let signed = sign_prepared_contract(contract.clone(), &test_secret_key())
        .expect("sign the prepared contract");

    let body = serde_json::json!({
        "prepare_request": prepare_request,
        "prepare_response": prepare_response["response"],
        "request": {
            "contract": Base64(signed.contract).to_json().expect("base64 contract"),
            "sig": Base64(signed.sig).to_json().expect("base64 signature"),
            "sig_algorithm": signed.sig_algorithm,
            "deployer": Base64(signed.deployer).to_json().expect("base64 deployer"),
        },
        "token": prepare_response["token"],
    });

    (body, contract)
}
