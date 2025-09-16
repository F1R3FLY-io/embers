use firefly_client::ReadNodeClient;
use firefly_client::rendering::Render;

use crate::ai_agents_teams::blockchain::dtos;
use crate::ai_agents_teams::models::AgentsTeams;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/list_agents_team_versions.rho")]
struct ListAgentsTeamVersions {
    address: WalletAddress,
    id: String,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn list_agents_team_versions(
    address: WalletAddress,
    id: String,
    client: &ReadNodeClient,
) -> anyhow::Result<Option<AgentsTeams>> {
    record_trace!(address, id);

    let code = ListAgentsTeamVersions { address, id }.render()?;

    let agents_teams: Option<Vec<dtos::AgentsTeamHeader>> = client.get_data(code).await?;
    Ok(agents_teams.map(|mut agents_teams| {
        agents_teams.sort_by(|l, r| l.version.cmp(&r.version));
        AgentsTeams {
            agents_teams: agents_teams.into_iter().map(Into::into).collect(),
        }
    }))
}
