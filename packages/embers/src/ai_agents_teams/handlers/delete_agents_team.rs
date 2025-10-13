use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};

use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::DeleteAgentsTeamResp;
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/delete_agents_team.rho")]
struct DeleteAgentsTeam {
    env_uri: Uri,
    id: String,
}

impl AgentsTeamsService {
    #[tracing::instrument(level = "info", skip(self), err(Debug), ret(Debug, level = "trace"))]
    pub async fn prepare_delete_agents_team_contract(
        &self,
        id: String,
    ) -> anyhow::Result<DeleteAgentsTeamResp> {
        let contract = DeleteAgentsTeam {
            env_uri: self.uri.clone(),
            id,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeleteAgentsTeamResp {
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
    pub async fn deploy_signed_delete_agents_team(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
