use std::time::Duration;

use anyhow::anyhow;
use firefly_client::models::{DeployId, SignedCode, Uri};
use firefly_client::rendering::Render;

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{RunReq, RunResp};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/run.rho")]
struct RunAgentsTeam {
    agents_team: Uri,
    prompt: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run_result.rho")]
struct GetAgentsTeamResult {
    deploy_id: DeployId,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_agents_team_contract(
        &self,
        request: RunReq,
    ) -> anyhow::Result<RunResp> {
        record_trace!(request);

        let contract = RunAgentsTeam {
            agents_team: request.agents_team,
            prompt: request.prompt,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RunResp {
            contract: prepare_for_signing()
                .code(contract)
                .phlo_limit(request.phlo_limit)
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
    pub async fn deploy_signed_run_agents_team(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<serde_json::Value> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;

        let result = self
            .observer_node_events
            .wait_for_deploy(&deploy_id, Duration::from_mins(1))
            .await;

        match result {
            Some(true) => return Err(anyhow!("deploy {deploy_id} errored on chain")),
            None => return Err(anyhow!("block is not finalized")),
            Some(false) => {}
        }

        let code = GetAgentsTeamResult { deploy_id }.render()?;
        self.read_client
            .get_data_with_retry(code, 5, Duration::from_millis(500))
            .await
            .map_err(Into::into)
    }
}
