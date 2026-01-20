use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::blockchain::agents::models;
use crate::domain::agents::AgentsService;
use crate::domain::agents::models::Agents;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "agents/list.rho")]
struct List {
    env_uri: Uri,
    address: WalletAddress,
}

impl AgentsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn list(&self, address: WalletAddress) -> anyhow::Result<Agents> {
        record_trace!(address);

        let code = List {
            env_uri: self.uri.clone(),
            address,
        }
        .render()?;
        self.read_client
            .get_data(code)
            .await
            .map(|agents: Vec<models::AgentHeader>| Agents {
                agents: agents.into_iter().map(Into::into).collect(),
            })
            .map_err(Into::into)
    }
}
