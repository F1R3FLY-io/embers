//! Full prepare → sign → send flows over HTTP: the signed bytes produced by
//! the client must travel through the JSON body, the JWT check, and the
//! handler unchanged, arriving at the write node byte-identically.

use poem::http::StatusCode;

use super::{prepare, signed_send_body, test_app};

#[tokio::test]
async fn agents_create_two_phase_flow_reaches_the_write_node() {
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({
        "name": "hermetic-agent",
        "description": "created through the in-process HTTP harness",
        "code": "new x in { x!(42) }",
    });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;

    let (send_body, contract) = signed_send_body(&prepare_request, &prepared);
    let response = client
        .post("/api/ai-agents/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::OK);

    let send: serde_json::Value = response.json().await.value().deserialize();
    assert_eq!(
        send["deploy_id"], "test-deploy-id",
        "send must answer with the mock deploy id"
    );
    assert_eq!(
        harness.write_node.deployed_contracts(),
        vec![contract],
        "the exact signed contract bytes must reach the write node"
    );
}

#[tokio::test]
async fn agents_teams_create_two_phase_flow_reaches_the_write_node() {
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({
        "name": "hermetic-team",
        "description": "created through the in-process HTTP harness",
    });
    let prepared = prepare(
        &client,
        "/api/ai-agents-teams/create/prepare",
        &prepare_request,
    )
    .await;

    let (send_body, contract) = signed_send_body(&prepare_request, &prepared);
    let response = client
        .post("/api/ai-agents-teams/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::OK);

    let send: serde_json::Value = response.json().await.value().deserialize();
    assert_eq!(
        send["deploy_id"], "test-deploy-id",
        "send must answer with the mock deploy id"
    );
    assert_eq!(
        harness.write_node.deployed_contracts(),
        vec![contract],
        "the exact signed contract bytes must reach the write node"
    );
}
