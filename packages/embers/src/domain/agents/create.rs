use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, SignedCode, Uri};
use firefly_client::rendering::Render;
use uuid::Uuid;

use crate::domain::agents::AgentsService;
use crate::domain::agents::models::{CreateReq, CreateResp};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents/create.rho")]
struct Create {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    code: Option<String>,
}

impl AgentsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_create_contract(&self, request: CreateReq) -> anyhow::Result<CreateResp> {
        record_trace!(request);

        let id = Uuid::new_v4();
        let version = Uuid::now_v7();

        let contract = Create {
            env_uri: self.uri.clone(),
            id,
            version,
            created_at: Utc::now(),
            name: request.name,
            description: request.description,
            shard: request.shard,
            logo: request.logo,
            code: request.code,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(CreateResp {
            id: id.into(),
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
    pub async fn deploy_signed_create(&self, contract: SignedCode) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        write_client.propose().await?;

        Ok(deploy_id)
    }
}
