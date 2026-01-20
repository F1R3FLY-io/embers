use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, SignedCode, Uri};
use firefly_client::rendering::Render;
use uuid::Uuid;

use crate::domain::common::{prepare_for_signing, record_trace};
use crate::domain::oslfs::OslfsService;
use crate::domain::oslfs::models::{SaveReq, SaveResp};

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/save.rho")]
struct Save {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    query: Option<String>,
}

impl OslfsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(id, request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_save_contract(
        &self,
        id: String,
        request: SaveReq,
    ) -> anyhow::Result<SaveResp> {
        record_trace!(id, request);

        let version = Uuid::now_v7();

        let contract = Save {
            env_uri: self.uri.clone(),
            id,
            version,
            created_at: Utc::now(),
            name: request.name,
            description: request.description,
            query: request.query,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(SaveResp {
            version: version.into(),
            contract: prepare_for_signing()
                .code(contract)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_save(&self, contract: SignedCode) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        write_client.propose().await?;
        Ok(deploy_id)
    }
}
