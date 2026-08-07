//! The JWT that binds a send to its prepare exchange: tampering with either
//! echoed payload, presenting an expired token, or reusing a token minted for
//! a different endpoint must all be refused, while a byte-identical replay is
//! deliberately accepted (the chain deduplicates).

use jsonwebtoken::{DecodingKey, EncodingKey, Validation};
use poem::http::StatusCode;

use super::{TEST_JWT_SECRET, prepare, signed_send_body, test_app};
use crate::api::common::Claims;

#[tokio::test]
async fn tampered_prepare_response_is_rejected() {
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({ "name": "tamper-response" });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;

    let (mut send_body, _contract) = signed_send_body(&prepare_request, &prepared);
    send_body["prepare_response"]["version"] =
        serde_json::json!("01890000-0000-7000-8000-000000000000");

    let response = client
        .post("/api/ai-agents/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        harness.write_node.deployed_contracts().is_empty(),
        "a rejected send must never reach the write node"
    );
}

#[tokio::test]
async fn tampered_prepare_request_is_rejected() {
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({ "name": "tamper-request" });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;

    let (mut send_body, _contract) = signed_send_body(&prepare_request, &prepared);
    send_body["prepare_request"]["name"] = serde_json::json!("someone-else");

    let response = client
        .post("/api/ai-agents/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        harness.write_node.deployed_contracts().is_empty(),
        "a rejected send must never reach the write node"
    );
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({ "name": "expired-token" });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;
    let (mut send_body, _contract) = signed_send_body(&prepare_request, &prepared);

    // Read the legit token's claims without enforcing expiry, then re-mint a
    // token carrying the same payload hash but an exp far in the past (well
    // beyond the default 60s validation leeway).
    let token = prepared["token"].as_str().expect("prepare issues a token");
    let mut validation = Validation::default();
    validation.validate_exp = false;
    let claims = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(TEST_JWT_SECRET),
        &validation,
    )
    .expect("decode the prepare token")
    .claims;

    let expired = Claims {
        exp: 1,
        hash: claims.hash,
    };
    send_body["token"] = serde_json::json!(
        jsonwebtoken::encode(
            &Default::default(),
            &expired,
            &EncodingKey::from_secret(TEST_JWT_SECRET),
        )
        .expect("encode the expired token")
    );

    let response = client
        .post("/api/ai-agents/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body = response
        .0
        .into_body()
        .into_string()
        .await
        .expect("read the rejection body");
    assert!(
        body.contains("ExpiredSignature"),
        "the rejection must name the expiry, got: {body}"
    );
    assert!(
        harness.write_node.deployed_contracts().is_empty(),
        "a rejected send must never reach the write node"
    );
}

#[tokio::test]
async fn token_minted_by_another_endpoint_is_rejected() {
    let (client, harness) = test_app();

    // A prepare/response pair from the agents endpoint parses under the teams
    // send schema too, but the token's hash covers the agents request and
    // response types, so the teams endpoint must refuse it.
    let prepare_request = serde_json::json!({ "name": "cross-endpoint" });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;
    let (send_body, _contract) = signed_send_body(&prepare_request, &prepared);

    let response = client
        .post("/api/ai-agents-teams/create/send")
        .body_json(&send_body)
        .send()
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body = response
        .0
        .into_body()
        .into_string()
        .await
        .expect("read the rejection body");
    assert!(
        body.contains("invalid token or request payloads"),
        "the rejection must report the hash mismatch, got: {body}"
    );
    assert!(
        harness.write_node.deployed_contracts().is_empty(),
        "a rejected send must never reach the write node"
    );
}

#[tokio::test]
async fn replayed_send_is_accepted_twice() {
    // The API layer accepts a byte-identical replay of a valid send by
    // design: the two-phase JWT only binds a send to its prepare exchange,
    // and the chain is the deduplicating verifier of record (the deploy id is
    // the contract signature, which the node refuses to admit twice). This
    // test pins that behavior so any future API-side replay window is a
    // deliberate change.
    let (client, harness) = test_app();

    let prepare_request = serde_json::json!({ "name": "replayed" });
    let prepared = prepare(&client, "/api/ai-agents/create/prepare", &prepare_request).await;
    let (send_body, contract) = signed_send_body(&prepare_request, &prepared);

    for round in 1..=2 {
        let response = client
            .post("/api/ai-agents/create/send")
            .body_json(&send_body)
            .send()
            .await;
        assert_eq!(
            response.0.status(),
            StatusCode::OK,
            "replay round {round} must be accepted"
        );
    }
    assert_eq!(
        harness.write_node.deployed_contracts(),
        vec![contract.clone(), contract],
        "both sends must reach the write node"
    );
}
