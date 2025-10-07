use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};
use uuid::Uuid;

use crate::ai_agents::handlers::AgentsService;
use crate::ai_agents::models::{CreateAgentReq, CreateAgentResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/create_agent.rho")]
struct CreateAgent {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    created_at: i64,
    name: String,
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
    pub async fn prepare_create_agent_contract(
        &self,
        request: CreateAgentReq,
    ) -> anyhow::Result<CreateAgentResp> {
        record_trace!(request);

        let id = Uuid::new_v4();
        let version = Uuid::now_v7();

        let contract = CreateAgent {
            env_uri: self.uri.clone(),
            id,
            version,
            created_at: Utc::now().timestamp(),
            name: request.name,
            shard: request.shard,
            logo: request.logo,
            code: request.code.map(|v| BASE64_STANDARD.encode(v)),
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(CreateAgentResp {
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
    pub async fn deploy_signed_create_agent(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
