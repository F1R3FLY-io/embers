use anyhow::Context;
use serde_json::Value;

use crate::errors::ReadNodeError;
use crate::models::{DeployId, ReadNodeExpr};

/// Deploy info returned by find_deploy_info.
pub struct DeployInfo {
    pub block_hash: String,
    pub block_number: u64,
    pub errored: bool,
}

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

    /// Check if a deploy has been included in a block, returning block hash and error status.
    ///
    /// Two-step lookup:
    /// 1. GET /api/deploy/{id} → returns the block containing the deploy
    /// 2. GET /api/block/{block_hash} → returns deploys[] with errored flag
    pub async fn find_deploy_info(
        &self,
        deploy_id: &DeployId,
    ) -> Result<Option<DeployInfo>, ReadNodeError> {
        // Step 1: Find which block contains the deploy
        let block_hash = match self.find_deploy(deploy_id).await? {
            Some(hash) => hash,
            None => return Ok(None),
        };

        // Step 2: Get block details to extract deploy's errored status
        let response = self
            .client
            .get(format!("{}/api/block/{}", self.url, block_hash))
            .send()
            .await?;

        if !response.status().is_success() {
            // Block found but can't get details — return with errored=false as fallback
            return Ok(Some(DeployInfo {
                block_hash,
                block_number: 0,
                errored: false,
            }));
        }

        let data: Value = response.json().await?;
        let block_number = data
            .pointer("/blockInfo/blockNumber")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Find our deploy in the block's deploy list by matching the deploy ID (sig)
        let deploy_id_str = deploy_id.to_string();
        let errored = data
            .get("deploys")
            .and_then(|v| v.as_array())
            .and_then(|deploys| {
                deploys.iter().find(|d| {
                    d.get("sig")
                        .and_then(|s| s.as_str())
                        .is_some_and(|s| s == deploy_id_str)
                })
            })
            .and_then(|d| d.get("errored"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Some(DeployInfo {
            block_hash,
            block_number,
            errored,
        }))
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

    /// Wait until the observer's last finalized block is at or past the given block number.
    /// This ensures explore-deploy will see state from that block.
    pub async fn wait_for_block(
        &self,
        block_number: u64,
        timeout: std::time::Duration,
    ) -> Result<(), ReadNodeError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Ok(current) = self.last_finalized_block_number().await {
                if current >= block_number {
                    return Ok(());
                }
                tracing::debug!(
                    target_block = block_number,
                    observer_block = current,
                    "waiting for observer to sync"
                );
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ReadNodeError::Api(
                    reqwest::StatusCode::REQUEST_TIMEOUT,
                    format!(
                        "observer did not reach block {block_number} within {:?}",
                        timeout
                    ),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// Get the last finalized block number from the observer.
    pub async fn last_finalized_block_number(&self) -> Result<u64, ReadNodeError> {
        let response = self
            .client
            .get(format!("{}/api/last-finalized-block", self.url))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(ReadNodeError::Api(status, body));
        }

        let data: Value = response.json().await?;
        data.pointer("/blockInfo/blockNumber")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                ReadNodeError::Deserialization(anyhow::anyhow!(
                    "missing blockInfo.blockNumber in last-finalized-block response"
                ))
            })
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
