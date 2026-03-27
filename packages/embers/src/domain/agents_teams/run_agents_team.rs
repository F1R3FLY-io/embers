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
        tracing::info!("run deploy_id: {deploy_id}");

        // Wait for finalization via HTTP polling
        let deadline = tokio::time::Instant::now() + self.observer_sync.finalization_timeout;
        loop {
            match self.read_client.find_deploy_info(&deploy_id).await {
                Ok(Some(info)) => {
                    if self.read_client.is_finalized(&info.block_hash).await.unwrap_or(false) {
                        if info.errored {
                            return Err(anyhow!("run deploy errored"));
                        }
                        tracing::info!("run deploy finalized at block {}", info.block_number);
                        break;
                    }
                }
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!("run deploy not finalized within timeout"));
            }
            tokio::time::sleep(self.observer_sync.interval).await;
        }

        // Retry read — observer explore-deploy may lag behind finalized state
        let code = GetAgentsTeamResult { deploy_id }.render()?;
        for attempt in 1..=self.observer_sync.read_after_finalize_attempts {
            match self.read_client.get_data::<serde_json::Value>(code.clone()).await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if attempt >= self.observer_sync.read_after_finalize_attempts {
                        return Err(err.into());
                    }
                    tracing::debug!(attempt, "run result not readable yet, retrying");
                    tokio::time::sleep(self.observer_sync.interval).await;
                }
            }
        }
        Err(anyhow!("run result not readable after retries"))
    }
}
