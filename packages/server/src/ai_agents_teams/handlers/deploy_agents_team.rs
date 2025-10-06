use anyhow::Context;
use firefly_client::models::SignedCode;
use uuid::Uuid;

use crate::ai_agents_teams::compilation::{parse, render_agent_team};
use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::{DeployAgentsTeamReq, DeployAgentsTeamResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_deploy_agents_team_contract(
        &self,
        request: DeployAgentsTeamReq,
    ) -> anyhow::Result<DeployAgentsTeamResp> {
        record_trace!(request);

        let (graph, phlo_limit) = match request {
            DeployAgentsTeamReq::AgentsTeam {
                id,
                version,
                address,
                phlo_limit,
            } => {
                let graph = self
                    .get_agents_team(address, id, version)
                    .await?
                    .context("agents team not found")?
                    .graph
                    .context("agents team has no graph")?;
                (graph, phlo_limit)
            }
            DeployAgentsTeamReq::Graph { graph, phlo_limit } => (graph, phlo_limit),
        };

        let name = Uuid::new_v4().to_string();

        let code = parse(&graph)?;
        let code = render_agent_team(&name, code)?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeployAgentsTeamResp {
            name,
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
    pub async fn deploy_signed_deploy_agents_team(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
