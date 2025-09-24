use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use firefly_client::rendering::Render;
use uuid::Uuid;

use crate::ai_agents_teams::models::{CreateAgentsTeamReq, CreateAgentsTeamResp, Graph};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/create_agents_team.rho")]
struct CreateAgentsTeam {
    id: Uuid,
    version: Uuid,
    name: String,
    shard: Option<String>,
    graph: Option<String>,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn prepare_create_agents_team_contract(
    request: CreateAgentsTeamReq,
    client: &mut WriteNodeClient,
) -> anyhow::Result<CreateAgentsTeamResp> {
    record_trace!(request);

    let id = Uuid::new_v4();
    let version = Uuid::now_v7();

    let contract = CreateAgentsTeam {
        id,
        version,
        name: request.name,
        shard: request.shard,
        graph: request.graph.map(Graph::graphl),
    }
    .render()?;

    let valid_after = client.get_head_block_index().await?;
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
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await?;
    Ok(())
}
