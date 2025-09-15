use firefly_client::ReadNodeClient;
use firefly_client::rendering::Render;

use crate::ai_agents_teams::blockchain::dtos;
use crate::ai_agents_teams::models::AgentsTeams;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/list_agents_teams.rho")]
struct ListAgentsTeams {
    address: WalletAddress,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn list_agents_teams(
    address: WalletAddress,
    client: &ReadNodeClient,
) -> anyhow::Result<AgentsTeams> {
    record_trace!(address);

    let code = ListAgentsTeams { address }.render()?;
    client
        .get_data(code)
        .await
        .map(|agents_teams: Vec<dtos::AgentsTeamHeader>| AgentsTeams {
            agents_teams: agents_teams.into_iter().map(Into::into).collect(),
        })
        .map_err(Into::into)
}
