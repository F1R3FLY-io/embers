use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::agents::models;
use crate::domain::agents::AgentsService;
use crate::domain::agents::models::Agent;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "agents/get.rho")]
struct Get {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

impl AgentsService {
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
    ) -> anyhow::Result<Option<Agent>> {
        record_trace!(address, id, version);

        let code = Get {
            env_uri: self.uri.clone(),
            address,
            id,
            version,
        }
        .render()?;

        let agent: Option<models::Agent> = self.read_client.get_data(code).await?;
        Ok(agent.map(Into::into))
    }
}
