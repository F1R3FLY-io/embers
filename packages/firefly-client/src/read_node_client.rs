use anyhow::Context;
use serde_json::Value;

use crate::models::ReadNodeExpr;

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

    pub async fn get_data<T>(&self, rholang_code: String) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut response_json = self.get_value(rholang_code).await?;

        let data_value = response_json
            .pointer_mut("/expr/0")
            .map(Value::take)
            .context("failed to extract data from response structure")?;

        let intermediate: ReadNodeExpr = serde_json::from_value(data_value)
            .context("failed to deserialize response data into intermediate type")?;

        serde_json::from_value(intermediate.into())
            .context("failed to deserialize response data into target type")
    }

    async fn get_value(&self, rholang_code: String) -> anyhow::Result<Value> {
        self.client
            .post(format!("{}/api/explore-deploy", self.url))
            .body(rholang_code)
            .header("Content-Type", "text/plain")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("failed to parse responce as json")
    }
}
