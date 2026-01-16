use poem_openapi::OpenApi;

use crate::api::common::ApiTags;

#[derive(Debug, Clone)]
pub struct Service;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/service", tag = ApiTags::Service)]
impl Service {
    #[oai(path = "/ready", method = "get")]
    async fn ready(&self) -> poem::Result<()> {
        Ok(())
    }
}
