use firefly_client::rendering::Uri;
use firefly_client::{ReadNodeClient, WriteNodeClient};

mod create_agents_team;
mod deploy_agents_team;
mod get_agents_team;
mod list_agents_team_versions;
mod list_agents_teams;
mod run_agents_team;
mod save_agents_team;

#[derive(Clone)]
pub struct AgentsTeamsService {
    pub uri: Uri,
    pub write_client: WriteNodeClient,
    pub read_client: ReadNodeClient,
}
