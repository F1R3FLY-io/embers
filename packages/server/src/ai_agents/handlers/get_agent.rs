use askama::Template;
use firefly_client::ReadNodeClient;

use crate::ai_agents::models::Agent;
use crate::common::rendering::RhoValue;
use crate::common::tracing::record_trace;
use crate::wallets::models::WalletAddress;

#[derive(Template)]
#[template(path = "ai_agents/get_agent.rho", escape = "none")]
struct GetAgent {
    address: RhoValue<WalletAddress>,
    id: RhoValue<String>,
    version: RhoValue<String>,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id, version),
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
        address: address.into(),
        id: id.into(),
        version: version.into(),
    }
    .render()
    .unwrap();

    client.get_data(code).await
}
