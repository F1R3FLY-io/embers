use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::AgentsTeams;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/list_agents_teams.rho")]
struct ListAgentsTeams {
    env_uri: Uri,
    address: WalletAddress,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn list_agents_teams(&self, address: WalletAddress) -> anyhow::Result<AgentsTeams> {
        record_trace!(address);

        let code = ListAgentsTeams {
            env_uri: self.uri.clone(),
            address,
        }
        .render()?;
        self.read_client
            .get_data(code)
            .await
            .map(|agents_teams: Vec<models::AgentsTeamHeader>| AgentsTeams {
                agents_teams: agents_teams.into_iter().map(Into::into).collect(),
            })
            .map_err(Into::into)
    }
}
