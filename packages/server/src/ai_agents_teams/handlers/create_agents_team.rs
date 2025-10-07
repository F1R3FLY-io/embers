use chrono::Utc;
use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};
use uuid::Uuid;

use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::{CreateAgentsTeamReq, CreateAgentsTeamResp, Graph};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/create_agents_team.rho")]
struct CreateAgentsTeam {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    name: String,
    shard: Option<String>,
    created_at: i64,
    graph: Option<String>,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_create_agents_team_contract(
        &self,
        request: CreateAgentsTeamReq,
    ) -> anyhow::Result<CreateAgentsTeamResp> {
        record_trace!(request);

        let id = Uuid::new_v4();
        let version = Uuid::now_v7();

        let contract = CreateAgentsTeam {
            env_uri: self.uri.clone(),
            id,
            version,
            name: request.name,
            shard: request.shard,
            created_at: Utc::now().timestamp(),
            graph: request.graph.map(Graph::graphl),
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(CreateAgentsTeamResp {
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
    pub async fn deploy_signed_create_agents_team(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
