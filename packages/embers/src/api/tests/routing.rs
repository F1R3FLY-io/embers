//! Routing behavior of the assembled app: the `MaybeNotFound` 404 mapping,
//! trailing-slash normalization, the readiness probe, and the served
//! `OpenAPI` spec.

use firefly_client::models::WalletAddress;
use poem::http::StatusCode;

use super::test_app;
use crate::domain::test_helpers::test_public_key;

#[tokio::test]
async fn unknown_agent_version_maps_to_empty_not_found() {
    let (client, _harness) = test_app();

    // The mock read node has no canned responses, so the lookup comes back
    // empty and the handler's `MaybeNotFound` maps it to a bodyless 404.
    let address: String = WalletAddress::from(test_public_key()).into();
    let response = client
        .get(format!(
            "/api/ai-agents/{address}/missing-id/versions/missing-version"
        ))
        .send()
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
    response.assert_text("").await;
}

#[tokio::test]
async fn trailing_slash_is_normalized() {
    let (client, _harness) = test_app();

    let response = client.get("/api/service/ready/").send().await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn service_ready_answers_ok() {
    let (client, _harness) = test_app();

    let response = client.get("/api/service/ready").send().await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn swagger_openapi_spec_is_served() {
    let (client, _harness) = test_app();

    let response = client.get("/swagger-ui/openapi.json").send().await;
    response.assert_status(StatusCode::OK);
    let body = response
        .0
        .into_body()
        .into_string()
        .await
        .expect("read the served spec");
    assert!(
        body.contains("openapi"),
        "the served spec must contain the openapi marker"
    );
}
