use firefly_client::ReadNodeClient;
use firefly_client::rendering::Render;

use crate::ai_agents_teams::blockchain::dtos;
use crate::ai_agents_teams::models::AgentsTeam;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/get_agents_team.rho")]
struct GetAgentsTeam {
    address: WalletAddress,
    id: String,
    version: String,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id, version),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn get_agents_team(
    address: WalletAddress,
    id: String,
    version: String,
    client: &ReadNodeClient,
) -> anyhow::Result<Option<AgentsTeam>> {
    record_trace!(address, id, version);

    let code = GetAgentsTeam {
        address,
        id,
        version,
    }
    .render()?;

    let agents_team: Option<dtos::AgentsTeam> = client.get_data(code).await?;
    Ok(agents_team.map(Into::into))
}
