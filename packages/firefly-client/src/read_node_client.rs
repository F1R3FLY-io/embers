use anyhow::Context;
use serde_json::Value;

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

    pub async fn get_data<T>(&self, path: &str, rholang_code: String) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut response_json = self.send_contract(rholang_code).await?;

        let data_value = response_json
            .pointer_mut(path)
            .map(Value::take)
            .context("failed to extract data from response structure")?;

        serde_json::from_value(data_value)
            .context("failed to deserialize response data into target type")
    }

    async fn send_contract(&self, rholang_code: String) -> anyhow::Result<Value> {
        let url = format!("{}/api/explore-deploy", self.url);
        self.client
            .post(url)
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
