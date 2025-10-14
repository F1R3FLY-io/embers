use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};

use crate::ai_agents::handlers::AgentsService;
use crate::ai_agents::models::DeleteAgentResp;
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/delete_agent.rho")]
struct DeleteAgent {
    env_uri: Uri,
    id: String,
}

impl AgentsService {
    #[tracing::instrument(level = "info", skip(self), err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_delete_agent_contract(
        &self,
        id: String,
    ) -> anyhow::Result<DeleteAgentResp> {
        let contract = DeleteAgent {
            env_uri: self.uri.clone(),
            id,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeleteAgentResp {
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
    pub async fn deploy_signed_delete_agent(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
