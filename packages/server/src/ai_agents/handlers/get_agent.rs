use firefly_client::ReadNodeClient;
use firefly_client::rendering::Render;

use crate::ai_agents::blockchain::dtos;
use crate::ai_agents::models::Agent;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/get_agent.rho")]
struct GetAgent {
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
pub async fn get_agent(
    address: WalletAddress,
    id: String,
    version: String,
    client: &ReadNodeClient,
) -> anyhow::Result<Option<Agent>> {
    record_trace!(address, id, version);

    let code = GetAgent {
        address,
        id,
        version,
    }
    .render()?;

    let agent: Option<dtos::Agent> = client.get_data(code).await?;
    Ok(agent.map(Into::into))
}
