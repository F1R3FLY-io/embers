use poem_openapi::OpenApi;

use crate::api::common::ApiTags;

#[derive(Debug, Clone)]
pub struct ServiceApi;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/service", tag = ApiTags::Service)]
impl ServiceApi {
    #[oai(path = "/ready", method = "get")]
    async fn ready(&self) -> poem::Result<()> {
        Ok(())
    }
}
