use firefly_client::ReadNodeClient;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;
use serde::Serialize;

use crate::api::common::ApiTags;

#[derive(Debug, Clone, Serialize, poem_openapi::Object)]
pub struct DeployStatus {
    /// Whether the deploy has been included in a block
    pub found: bool,
    /// Block hash containing the deploy (if found)
    pub block_hash: Option<String>,
    /// Whether the block is finalized
    pub finalized: bool,
}

#[derive(Debug, Clone)]
pub struct ServiceApi;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/service", tag = ApiTags::Service)]
impl ServiceApi {
    #[oai(path = "/ready", method = "get")]
    async fn ready(&self) -> poem::Result<()> {
        Ok(())
    }

    /// Check deploy finalization status via observer HTTP polling.
    /// Returns whether the deploy is in a block and if that block is finalized.
    #[oai(path = "/deploys/:deploy_id/status", method = "get")]
    async fn deploy_status(
        &self,
        Path(deploy_id): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<DeployStatus>> {
        let deploy_id_typed = deploy_id.into();

        // Check if deploy is in a block
        let block_hash = match read_client.find_deploy(&deploy_id_typed).await {
            Ok(Some(hash)) => hash,
            Ok(None) => {
                return Ok(Json(DeployStatus {
                    found: false,
                    block_hash: None,
                    finalized: false,
                }));
            }
            Err(_) => {
                return Ok(Json(DeployStatus {
                    found: false,
                    block_hash: None,
                    finalized: false,
                }));
            }
        };

        // Check if block is finalized
        let finalized = read_client
            .is_finalized(&block_hash)
            .await
            .unwrap_or(false);

        Ok(Json(DeployStatus {
            found: true,
            block_hash: Some(block_hash),
            finalized,
        }))
    }
}
