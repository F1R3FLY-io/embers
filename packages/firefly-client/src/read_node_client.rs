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

        let intermediate: ReadNodeExpr =
            serde_json::from_value(data_value).map_err(ReadNodeError::InvalidIntermediateModel)?;

        serde_json::from_value(intermediate.into()).map_err(ReadNodeError::InvalidFinalModel)
    }

    async fn explore_deploy(&self, rholang_code: String) -> Result<Value, ReadNodeError> {
        self.client
            .post(format!("{}/api/explore-deploy", self.url))
            .body(rholang_code)
            .header("Content-Type", "text/plain")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }
}
