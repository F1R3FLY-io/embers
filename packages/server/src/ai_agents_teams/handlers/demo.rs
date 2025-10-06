use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;
use firefly_client::models::DeployId;
use firefly_client::rendering::Render;
use secp256k1::SecretKey;

use crate::ai_agents_teams::handlers::AgentsTeamsService;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/run_demo.rho")]
struct RunAiAgentsTeamsDemo {
    name: String,
    prompt: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/get_demo_result.rho")]
struct GetAiAgentsTeamsDemoResult {
    deploy_id: DeployId,
}

impl AgentsTeamsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn run_demo(
        &mut self,
        name: String,
        prompt: String,
    ) -> anyhow::Result<serde_json::Value> {
        let key =
            SecretKey::from_str("6a786ec387aff99fcce1bd6faa35916bfad3686d5c98e90a89f77670f535607c")
                .unwrap();

        let deploy_data = RunAiAgentsTeamsDemo {
            name,
            prompt: prompt.clone(),
        }
        .builder()?
        .phlo_limit(500_000_000)
        .build();

        let deploy_id = self.write_client.deploy(&key, deploy_data).await?;
        let block_hash = self.write_client.propose().await?;

        let finalized = self
            .read_client
            .wait_finalization(block_hash, Duration::from_secs(1500))
            .await?;

        if !finalized {
            return Err(anyhow!("block is not finalized"));
        }

        let code = GetAiAgentsTeamsDemoResult { deploy_id }.render()?;
        self.read_client.get_data(code).await.map_err(Into::into)
    }
}
