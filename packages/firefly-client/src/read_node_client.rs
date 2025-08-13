use std::time::Duration;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use futures::future::Either;
use serde_json::Value;

use crate::errors::ReadNodeError;
use crate::models::{BlockId, ReadNodeExpr};

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

    async fn explore_deploy(&self, rholang_code: String) -> Result<Value, ReadNodeError> {
        let request = self
            .client
            .post(format!("{}/api/explore-deploy", self.url))
            .body(rholang_code)
            .header("Content-Type", "text/plain")
            .send()
            .await?;

        if !request.status().is_success() {
            let status = request.status();
            let body = request.text().await?;
            return Err(ReadNodeError::Api(status, body));
        }

        request.json().await.map_err(Into::into)
    }

    pub async fn wait_finalization(
        &self,
        block_id: BlockId,
        total_delay: Duration,
    ) -> anyhow::Result<bool> {
        let result: Result<bool, Either<bool, anyhow::Error>> = (|| async {
            let finalized: bool = self
                .client
                .get(format!("{}/api/is-finalized/{block_id}", self.url))
                .send()
                .await
                .map_err(|err| Either::Right(err.into()))?
                .error_for_status()
                .map_err(|err| Either::Right(err.into()))?
                .json()
                .await
                .map_err(|err| Either::Right(err.into()))?;

            if finalized {
                Ok(finalized)
            } else {
                Err(Either::Left(finalized))
            }
        })
        .retry(
            ExponentialBuilder::default()
                .without_max_times()
                .with_total_delay(Some(total_delay)),
        )
        .await;

        match result {
            Ok(finalized) => Ok(finalized),
            Err(Either::Left(finalized)) => Ok(finalized),
            Err(Either::Right(err)) => Err(err),
        }
    }
}
