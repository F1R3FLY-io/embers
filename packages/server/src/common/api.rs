use poem_openapi::OpenApi;

use crate::common::api::dtos::ApiTags;

pub mod dtos;

#[derive(Debug, Clone)]
pub struct Service;

#[OpenApi(prefix_path = "/service", tag = ApiTags::Service)]
impl Service {
    #[oai(path = "/ready", method = "get")]
    async fn ready(&self) -> poem::Result<()> {
        Ok(())
    }
}
