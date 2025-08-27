use firefly_client::models::SignedCode;
use firefly_client::{WriteNodeClient, template};
use uuid::Uuid;

use crate::ai_agents_teams::blockchain::dtos::Graph;
use crate::ai_agents_teams::models::{SaveAgentsTeamReq, SaveAgentsTeamResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

template! {
    #[template(path = "ai_agents_teams/save_agents_team.rho")]
    #[derive(Debug, Clone)]
    struct SaveAgentsTeam {
        id: String,
        version: Uuid,
        name: String,
        shard: Option<String>,
        graph: Option<String>,
        graph_ast: Option<Graph>,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(id, request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub fn prepare_save_agents_team_contract(
    id: String,
    request: SaveAgentsTeamReq,
) -> anyhow::Result<SaveAgentsTeamResp> {
    record_trace!(id, request);

    let version = Uuid::now_v7();

    let contract = SaveAgentsTeam {
        id,
        version,
        name: request.name,
        shard: request.shard,
        graph: request.graph,
        graph_ast: request.graph_ast.map(Into::into),
    }
    .render()?;

    Ok(SaveAgentsTeamResp {
        version: version.into(),
        contract: prepare_for_signing(contract),
    })
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_save_agents_team(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await?;
    Ok(())
}
