use chrono::Utc;
use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};
use uuid::Uuid;

use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::{Graph, SaveAgentsTeamReq, SaveAgentsTeamResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/save_agents_team.rho")]
struct SaveAgentsTeam {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: i64,
    name: String,
    shard: Option<String>,
    logo: Option<String>,
    graph: Option<String>,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(id, request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_save_agents_team_contract(
        &self,
        id: String,
        request: SaveAgentsTeamReq,
    ) -> anyhow::Result<SaveAgentsTeamResp> {
        record_trace!(id, request);

        let version = Uuid::now_v7();

        let contract = SaveAgentsTeam {
            env_uri: self.uri.clone(),
            id,
            version,
            created_at: Utc::now().timestamp(),
            name: request.name,
            shard: request.shard,
            logo: request.logo,
            graph: request.graph.map(Graph::graphl),
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(SaveAgentsTeamResp {
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
    pub async fn deploy_signed_save_agents_team(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
