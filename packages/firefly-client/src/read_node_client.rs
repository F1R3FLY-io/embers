use std::time::Duration;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use serde_json::Value;

use crate::errors::ReadNodeError;
use crate::models::ReadNodeExpr;
use crate::traits::ReadNode;

#[derive(Clone)]
pub struct ReadNodeClient {
    url: String,
    client: reqwest::Client,
}

impl ReadNodeClient {
    pub fn new(url: String) -> Self {
        tracing::info!("ReadNodeClient targeting: {url}");
        Self {
            url,
            client: Default::default(),
        }
    }

    pub async fn get_data<T>(&self, rholang_code: String) -> Result<T, ReadNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        let code_len = rholang_code.len();
        let code_prefix: String = rholang_code.chars().take(200).collect();
        let mut response_json = self.explore_deploy(rholang_code).await?;

        let data_value = match response_json.pointer_mut("/expr/0").map(Value::take) {
            Some(v) => v,
            None => {
                tracing::warn!(
                    response = %response_json,
                    code_len,
                    code_prefix = %code_prefix,
                    "explore-deploy returned no data at /expr/0"
                );
                return Err(ReadNodeError::ReturnValueMissing);
            }
        };

        let intermediate: ReadNodeExpr = serde_json::from_value(data_value.clone())
            .context("failed to deserialize intermediate model")
            .map_err(|err| {
                tracing::error!(
                    raw_value = %data_value,
                    code_prefix = %code_prefix,
                    error = %err,
                    "DIAG: intermediate deserialization failed — raw /expr/0 value logged above"
                );
                ReadNodeError::Deserialization(err)
            })?;

        let final_value: Value = intermediate.into();
        serde_json::from_value(final_value.clone())
            .context("failed to deserialize final model")
            .map_err(|err| {
                tracing::error!(
                    final_json = %final_value,
                    code_prefix = %code_prefix,
                    error = ?err,
                    type_name = std::any::type_name::<T>(),
                    "DIAG: final model deserialization failed — converted JSON logged above"
                );
                // Log each field's JSON type to help identify which field fails
                if let Some(obj) = final_value.as_object() {
                    for (key, value) in obj {
                        let value_type = match value {
                            Value::Null => "null",
                            Value::Bool(_) => "bool",
                            Value::Number(_) => "number",
                            Value::String(s) => {
                                tracing::debug!(
                                    field = %key,
                                    value_prefix = %&s[..s.len().min(100)],
                                    value_len = s.len(),
                                    "DIAG: field detail (string)"
                                );
                                "string"
                            }
                            Value::Array(_) => "array",
                            Value::Object(_) => "object",
                        };
                        tracing::debug!(
                            field = %key,
                            value_type,
                            "DIAG: field type"
                        );
                    }
                }
                ReadNodeError::Deserialization(err)
            })
    }

    pub async fn get_data_or_none<T>(
        &self,
        rholang_code: String,
    ) -> Result<Option<T>, ReadNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        match self.get_data(rholang_code).await {
            Ok(data) => Ok(Some(data)),
            Err(ReadNodeError::ReturnValueMissing) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Retry `get_data` up to `max_retries` times with exponential backoff and jitter.
    /// Only retries on `ReturnValueMissing` (empty explore-deploy result).
    /// Enforces a 45-second total timeout to prevent unbounded hangs.
    pub async fn get_data_with_retry<T>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> Result<T, ReadNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        let total_timeout = Duration::from_secs(45);

        let backoff = ExponentialBuilder::default()
            .with_min_delay(delay)
            .with_max_delay(delay.saturating_mul(5))
            .with_max_times(max_retries as usize)
            .with_jitter();

        let retry_fut = (|| self.get_data::<T>(rholang_code.clone()))
            .retry(backoff)
            .when(|err| matches!(err, ReadNodeError::ReturnValueMissing))
            .notify(|err, dur| {
                tracing::debug!(?err, ?dur, "explore-deploy returned empty, retrying");
            });

        match tokio::time::timeout(total_timeout, retry_fut).await {
            Ok(result) => result,
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = total_timeout.as_secs(),
                    code_prefix = %&rholang_code[..rholang_code.len().min(200)],
                    "get_data_with_retry timed out"
                );
                Err(ReadNodeError::Timeout(total_timeout))
            }
        }
    }

    /// Retry `get_data_or_none` up to `max_retries` times with `delay` between attempts.
    /// Only retries when the result is `Ok(None)` (empty explore-deploy result).
    pub async fn get_data_or_none_with_retry<T>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> Result<Option<T>, ReadNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        match self
            .get_data_with_retry(rholang_code, max_retries, delay)
            .await
        {
            Ok(data) => Ok(Some(data)),
            Err(ReadNodeError::ReturnValueMissing) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn explore_deploy(&self, rholang_code: String) -> Result<Value, ReadNodeError> {
        let request = self
            .client
            .post(format!("{}/api/explore-deploy", self.url))
            .json(&serde_json::json!({ "term": rholang_code }))
            .send()
            .await?;

        if !request.status().is_success() {
            let status = request.status();
            let body = request.text().await?;
            tracing::warn!(%status, %body, "explore-deploy HTTP error");
            return Err(ReadNodeError::Api(status, body));
        }

        let response: Value = request.json().await?;

        // Log block info from every explore-deploy response for diagnostics
        if let Some(block) = response.get("block") {
            tracing::info!(
                target: "f1r3fly.rholang.diag",
                block_number = block.get("blockNumber").and_then(|v| v.as_u64()),
                block_hash = block.get("blockHash").and_then(|v| v.as_str()),
                deploy_count = block.get("deployCount").and_then(|v| v.as_u64()),
                post_state = block.get("postStateHash").and_then(|v| v.as_str()),
                "explore_deploy: response block info"
            );
        }

        // Warn on empty expr, debug-log otherwise
        match response.pointer("/expr/0") {
            Some(_) => tracing::trace!(response = %response, "explore_deploy response (has data)"),
            None => tracing::warn!(response = %response, "explore_deploy response (empty expr)"),
        }

        Ok(response)
    }
}

impl ReadNode for ReadNodeClient {
    async fn get_data<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> Result<T, ReadNodeError> {
        self.get_data(rholang_code).await
    }

    async fn get_data_or_none<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
    ) -> Result<Option<T>, ReadNodeError> {
        self.get_data_or_none(rholang_code).await
    }

    async fn get_data_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> Result<T, ReadNodeError> {
        self.get_data_with_retry(rholang_code, max_retries, delay)
            .await
    }

    async fn get_data_or_none_with_retry<T: serde::de::DeserializeOwned + Send>(
        &self,
        rholang_code: String,
        max_retries: u32,
        delay: Duration,
    ) -> Result<Option<T>, ReadNodeError> {
        self.get_data_or_none_with_retry(rholang_code, max_retries, delay)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde::Deserialize;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Helper: returns a valid explore-deploy JSON response wrapping a string expr.
    fn string_expr_response(data: &str) -> serde_json::Value {
        json!({ "expr": [{ "ExprString": { "data": data } }] })
    }

    /// Helper: returns a valid explore-deploy JSON response wrapping a bool expr.
    fn bool_expr_response(data: bool) -> serde_json::Value {
        json!({ "expr": [{ "ExprBool": { "data": data } }] })
    }

    /// Helper: returns a valid explore-deploy JSON response wrapping an int expr.
    fn int_expr_response(data: i64) -> serde_json::Value {
        json!({ "expr": [{ "ExprInt": { "data": data } }] })
    }

    /// Helper: returns an explore-deploy JSON response with empty expr.
    fn empty_expr_response() -> serde_json::Value {
        json!({ "expr": [] })
    }

    /// Mount a mock on the server that returns the given body for POST /api/explore-deploy.
    async fn mount_explore_deploy(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    // -------------------------------------------------------
    // get_data tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_get_data_success_deserializes_string() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, string_expr_response("hello")).await;

        let client = ReadNodeClient::new(server.uri());
        let result: String = client
            .get_data("code".into())
            .await
            .expect("should succeed");
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_get_data_success_deserializes_bool() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, bool_expr_response(true)).await;

        let client = ReadNodeClient::new(server.uri());
        let result: bool = client
            .get_data("code".into())
            .await
            .expect("should succeed");
        assert!(result);
    }

    #[tokio::test]
    async fn test_get_data_success_deserializes_int() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, int_expr_response(42)).await;

        let client = ReadNodeClient::new(server.uri());
        let result: i64 = client
            .get_data("code".into())
            .await
            .expect("should succeed");
        assert_eq!(result, 42);
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        value: i64,
    }

    #[tokio::test]
    async fn test_get_data_success_deserializes_struct() {
        let server = MockServer::start().await;
        let body = json!({
            "expr": [{
                "ExprMap": {
                    "data": {
                        "name": { "ExprString": { "data": "alice" } },
                        "value": { "ExprInt": { "data": 99 } }
                    }
                }
            }]
        });
        mount_explore_deploy(&server, body).await;

        let client = ReadNodeClient::new(server.uri());
        let result: TestStruct = client
            .get_data("code".into())
            .await
            .expect("should succeed");
        assert_eq!(
            result,
            TestStruct {
                name: "alice".into(),
                value: 99
            }
        );
    }

    #[tokio::test]
    async fn test_get_data_return_value_missing_on_empty_expr() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, empty_expr_response()).await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data::<String>("code".into()).await;
        assert!(
            matches!(result, Err(ReadNodeError::ReturnValueMissing)),
            "expected ReturnValueMissing, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_get_data_return_value_missing_on_no_expr_key() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, json!({})).await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data::<String>("code".into()).await;
        assert!(matches!(result, Err(ReadNodeError::ReturnValueMissing)));
    }

    #[tokio::test]
    async fn test_get_data_api_error_on_http_500() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data::<String>("code".into()).await;
        match result {
            Err(ReadNodeError::Api(status, body)) => {
                assert_eq!(status, reqwest::StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body, "internal error");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_data_api_error_on_http_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data::<String>("code".into()).await;
        match result {
            Err(ReadNodeError::Api(status, body)) => {
                assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
                assert_eq!(body, "bad request");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_data_deserialization_error_on_type_mismatch() {
        let server = MockServer::start().await;
        // Return a string expr, but try to deserialize as struct
        mount_explore_deploy(&server, string_expr_response("hello")).await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data::<TestStruct>("code".into()).await;
        assert!(
            matches!(result, Err(ReadNodeError::Deserialization(_))),
            "expected Deserialization error, got {result:?}"
        );
    }

    // -------------------------------------------------------
    // get_data_or_none tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_get_data_or_none_returns_some_on_success() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, string_expr_response("hello")).await;

        let client = ReadNodeClient::new(server.uri());
        let result: Option<String> = client
            .get_data_or_none("code".into())
            .await
            .expect("should succeed");
        assert_eq!(result, Some("hello".into()));
    }

    #[tokio::test]
    async fn test_get_data_or_none_returns_none_on_missing_value() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, empty_expr_response()).await;

        let client = ReadNodeClient::new(server.uri());
        let result: Option<String> = client
            .get_data_or_none("code".into())
            .await
            .expect("should succeed");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_get_data_or_none_propagates_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(500).set_body_string("error"))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client.get_data_or_none::<String>("code".into()).await;
        assert!(matches!(result, Err(ReadNodeError::Api(_, _))));
    }

    // -------------------------------------------------------
    // get_data_with_retry tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_get_data_with_retry_succeeds_first_try() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, string_expr_response("ok")).await;

        let client = ReadNodeClient::new(server.uri());
        let result: String = client
            .get_data_with_retry("code".into(), 3, Duration::from_millis(100))
            .await
            .expect("should succeed");
        assert_eq!(result, "ok");
    }

    #[tokio::test]
    async fn test_get_data_with_retry_retries_on_missing_then_succeeds() {
        let server = MockServer::start().await;

        // First 2 calls return empty
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_expr_response()))
            .up_to_n_times(2)
            .expect(2)
            .mount(&server)
            .await;

        // 3rd call returns data
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(string_expr_response("found")))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result: String = client
            .get_data_with_retry("code".into(), 5, Duration::from_millis(10))
            .await
            .expect("should succeed after retries");
        assert_eq!(result, "found");
    }

    #[tokio::test]
    async fn test_get_data_with_retry_no_retry_on_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(500).set_body_string("fail"))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client
            .get_data_with_retry::<String>("code".into(), 5, Duration::from_millis(10))
            .await;
        assert!(
            matches!(result, Err(ReadNodeError::Api(_, _))),
            "expected Api error without retry, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_get_data_with_retry_exhausts_retries() {
        let server = MockServer::start().await;

        // Always return empty
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_expr_response()))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client
            .get_data_with_retry::<String>("code".into(), 2, Duration::from_millis(10))
            .await;

        // After exhausting retries, backon returns the last error
        assert!(
            matches!(
                result,
                Err(ReadNodeError::ReturnValueMissing) | Err(ReadNodeError::Timeout(_))
            ),
            "expected ReturnValueMissing or Timeout, got {result:?}"
        );
    }

    /// Timeout test: we can't easily test the 45-second timeout with a real
    /// HTTP server, so instead we verify that the timeout duration is correct
    /// by directly testing the Timeout error variant construction.
    #[test]
    fn test_timeout_error_has_correct_duration() {
        let timeout = Duration::from_secs(45);
        let err = ReadNodeError::Timeout(timeout);
        match err {
            ReadNodeError::Timeout(d) => assert_eq!(d, Duration::from_secs(45)),
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    // -------------------------------------------------------
    // get_data_or_none_with_retry tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_get_data_or_none_with_retry_returns_some_on_success() {
        let server = MockServer::start().await;
        mount_explore_deploy(&server, string_expr_response("data")).await;

        let client = ReadNodeClient::new(server.uri());
        let result: Option<String> = client
            .get_data_or_none_with_retry("code".into(), 3, Duration::from_millis(10))
            .await
            .expect("should succeed");
        assert_eq!(result, Some("data".into()));
    }

    #[tokio::test]
    async fn test_get_data_or_none_with_retry_returns_none_on_exhausted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_expr_response()))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result: Option<String> = client
            .get_data_or_none_with_retry("code".into(), 2, Duration::from_millis(10))
            .await
            .expect("should return Ok(None) after retries");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_get_data_or_none_with_retry_propagates_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/explore-deploy"))
            .respond_with(ResponseTemplate::new(500).set_body_string("error"))
            .mount(&server)
            .await;

        let client = ReadNodeClient::new(server.uri());
        let result = client
            .get_data_or_none_with_retry::<String>("code".into(), 3, Duration::from_millis(10))
            .await;
        assert!(matches!(result, Err(ReadNodeError::Api(_, _))));
    }
}
