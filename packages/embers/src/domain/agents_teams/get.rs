use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::AgentsTeam;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/get_agents_team.rho")]
struct Get {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id, version),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn get(
        &self,
        address: WalletAddress,
        id: String,
        version: String,
    ) -> anyhow::Result<Option<AgentsTeam>> {
        record_trace!(address, id, version);

        let code = Get {
            env_uri: self.uri.clone(),
            address,
            id,
            version,
        }
        .render()?;

        let agents_team: Option<models::AgentsTeam> = self.read_client.get_data(code).await?;
        Ok(agents_team.map(Into::into))
    }
}
