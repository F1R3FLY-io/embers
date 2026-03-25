use anyhow::Context;
use serde_json::Value;

use crate::errors::ReadNodeError;
use crate::models::{DeployId, ReadNodeExpr};

#[derive(Clone)]
pub struct ReadNodeClient {
    url: String,
    client: reqwest::Client,
}

impl ReadNodeClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Default::default(),
        }
    }

    pub async fn get_data<T>(&self, rholang_code: String) -> Result<T, ReadNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut response_json = self.explore_deploy(rholang_code).await?;

        tracing::debug!(
            "explore-deploy response keys: {:?}",
            response_json
                .as_object()
                .map(|o| o.keys().collect::<Vec<_>>())
        );
        tracing::debug!(
            "explore-deploy expr: {}",
            serde_json::to_string(&response_json.get("expr")).unwrap_or_default()
        );

        let data_value = response_json
            .pointer_mut("/expr/0")
            .map(Value::take)
            .ok_or(ReadNodeError::ReturnValueMissing)?;

        let intermediate: ReadNodeExpr = serde_json::from_value(data_value)
            .context("failed to deserialize intermediate model")
            .map_err(ReadNodeError::Deserialization)?;

        serde_json::from_value(intermediate.into())
            .context("failed to deserialize filed model")
            .map_err(ReadNodeError::Deserialization)
    }

    /// Check if a deploy has been included in a block.
    /// Returns the block hash if found, None if not yet included.
    pub async fn find_deploy(&self, deploy_id: &DeployId) -> Result<Option<String>, ReadNodeError> {
        let response = self
            .client
            .get(format!("{}/api/deploy/{}", self.url, deploy_id))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            // "deploy not found" style errors mean not yet in a block
            if body.contains("not found") || body.contains("NOT_FOUND") {
                return Ok(None);
            }
            return Err(ReadNodeError::Api(status, body));
        }

        let data: Value = response.json().await?;
        let block_hash = data
            .get("blockHash")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(block_hash)
    }

    /// Check if a block is finalized.
    pub async fn is_finalized(&self, block_hash: &str) -> Result<bool, ReadNodeError> {
        let response = self
            .client
            .get(format!("{}/api/is-finalized/{}", self.url, block_hash))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(ReadNodeError::Api(status, body));
        }

        Ok(response.json().await?)
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
            return Err(ReadNodeError::Api(status, body));
        }

        request.json().await.map_err(Into::into)
    }
}
