use firefly_client::ReadNodeClient;
use firefly_client::rendering::Render;

use crate::ai_agents::blockchain::dtos;
use crate::ai_agents::models::Agents;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/list_agents.rho")]
struct ListAgents {
    address: WalletAddress,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn list_agents(
    address: WalletAddress,
    client: &ReadNodeClient,
) -> anyhow::Result<Agents> {
    record_trace!(address);

    let code = ListAgents { address }.render()?;
    client
        .get_data(code)
        .await
        .map(|agents: Vec<dtos::AgentHeader>| Agents {
            agents: agents.into_iter().map(Into::into).collect(),
        })
        .map_err(Into::into)
}
