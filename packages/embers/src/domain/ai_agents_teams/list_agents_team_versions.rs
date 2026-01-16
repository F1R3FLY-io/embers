use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::ai_agents_teams::models;
use crate::domain::ai_agents_teams::AgentsTeamsService;
use crate::domain::ai_agents_teams::models::AgentsTeams;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/list_agents_team_versions.rho")]
struct ListAgentsTeamVersions {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn list_agents_team_versions(
        &self,
        address: WalletAddress,
        id: String,
    ) -> anyhow::Result<Option<AgentsTeams>> {
        record_trace!(address, id);

        let code = ListAgentsTeamVersions {
            env_uri: self.uri.clone(),
            address,
            id,
        }
        .render()?;

        let agents_teams: Option<Vec<models::AgentsTeamHeader>> =
            self.read_client.get_data(code).await?;
        Ok(agents_teams.map(|mut agents_teams| {
            agents_teams.sort_by(|l, r| l.version.cmp(&r.version));
            AgentsTeams {
                agents_teams: agents_teams.into_iter().map(Into::into).collect(),
            }
        }))
    }
}
