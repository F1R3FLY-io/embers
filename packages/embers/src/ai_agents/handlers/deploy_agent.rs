use anyhow::Context;
use firefly_client::models::SignedCode;

use crate::ai_agents::handlers::AgentsService;
use crate::ai_agents::models::{DeployAgentReq, DeployAgentResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

impl AgentsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_deploy_agent_contract(
        &self,
        request: DeployAgentReq,
    ) -> anyhow::Result<DeployAgentResp> {
        record_trace!(request);

        let (code, phlo_limit) = match request {
            DeployAgentReq::Agent {
                id,
                version,
                address,
                phlo_limit,
            } => {
                let code = self
                    .get_agent(address, id, version)
                    .await?
                    .context("agent not found")?
                    .code
                    .context("agent has no code")?;
                (code, phlo_limit)
            }
            DeployAgentReq::Code { code, phlo_limit } => (code, phlo_limit),
        };

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeployAgentResp {
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .phlo_limit(phlo_limit)
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
    pub async fn deploy_signed_deploy_agent(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
