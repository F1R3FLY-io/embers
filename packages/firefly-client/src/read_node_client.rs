use std::time::Duration;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use serde_json::Value;

use crate::errors::ReadNodeError;
use crate::models::ReadNodeExpr;

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

    pub async fn get_data_or_none<T>(&self, rholang_code: String) -> Result<Option<T>, ReadNodeError>
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
        match self.get_data_with_retry(rholang_code, max_retries, delay).await {
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
