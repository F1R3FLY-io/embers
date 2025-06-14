use askama::Template;
use firefly_client::ReadNodeClient;

use crate::ai_agents::models::Agents;
use crate::common::rendering::RhoValue;
use crate::common::tracing::record_trace;
use crate::wallets::models::WalletAddress;

#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents/list_agents.rho", escape = "none")]
struct ListAgents {
    address: RhoValue<WalletAddress>,
}

#[tracing::instrument(level = "info", skip_all, fields(address), ret(Debug, level = "trace"))]
pub async fn list_agents(
    address: WalletAddress,
    client: &ReadNodeClient,
) -> anyhow::Result<Agents> {
    record_trace!(address);

    let code = ListAgents {
        address: address.into(),
    }
    .render()
    .unwrap();

    client.get_data(code).await.map(|agents| Agents { agents })
}
