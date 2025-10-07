use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};
use uuid::Uuid;

use crate::ai_agents::handlers::AgentsService;
use crate::ai_agents::models::{SaveAgentReq, SaveAgentResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/save_agent.rho")]
struct SaveAgent {
    env_uri: Uri,
    id: String,
    version: Uuid,
    name: String,
    shard: Option<String>,
    created_at: i64,
    code: Option<String>,
}

impl AgentsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(id, request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_save_agent_contract(
        &self,
        id: String,
        request: SaveAgentReq,
    ) -> anyhow::Result<SaveAgentResp> {
        record_trace!(id, request);

        let version = Uuid::now_v7();

        let contract = SaveAgent {
            env_uri: self.uri.clone(),
            id,
            version,
            name: request.name,
            shard: request.shard,
            created_at: Utc::now().timestamp(),
            code: request.code.map(|v| BASE64_STANDARD.encode(v)),
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(SaveAgentResp {
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
    pub async fn deploy_signed_save_agent(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
