use firefly_client::{ReadNodeClient, template};

use crate::ai_agents::models::{AgentHeader, Agents};
use crate::common::tracing::record_trace;
use crate::wallets::models::WalletAddress;

template! {
    #[template(path = "ai_agents/list_agent_versions.rho")]
    #[derive(Debug, Clone)]
    struct ListAgentVersions {
        address: WalletAddress,
        id: String,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn list_agent_versions(
    address: WalletAddress,
    id: String,
    client: &ReadNodeClient,
) -> anyhow::Result<Option<Agents>> {
    record_trace!(address, id);

    let code = ListAgentVersions { address, id }.render()?;

    let agents: Option<Vec<AgentHeader>> = client.get_data(code).await?;
    Ok(agents.map(|mut agents| {
        agents.sort_by(|l, r| l.version.cmp(&r.version));
        Agents { agents }
    }))
}
