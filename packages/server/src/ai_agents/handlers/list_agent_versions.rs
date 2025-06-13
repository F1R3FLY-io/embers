use askama::Template;
use firefly_client::ReadNodeClient;

use crate::ai_agents::models::{AgentHeader, Agents};
use crate::common::rendering::RhoValue;
use crate::common::tracing::record_trace;
use crate::wallets::models::WalletAddress;

#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents/list_agent_versions.rho", escape = "none")]
struct ListAgentVersions {
    address: RhoValue<WalletAddress>,
    id: RhoValue<String>,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id),
    ret(Debug, level = "trace")
)]
pub async fn list_agent_versions(
    address: WalletAddress,
    id: String,
    client: &ReadNodeClient,
) -> anyhow::Result<Option<Agents>> {
    record_trace!(address, id);

    let code = ListAgentVersions {
        address: address.into(),
        id: id.into(),
    }
    .render()
    .unwrap();

    let agents: Option<Vec<AgentHeader>> = client.get_data(code).await?;
    Ok(agents.map(|mut agents| {
        agents.sort_by(|l, r| l.version.cmp(&r.version));
        Agents { agents }
    }))
}
