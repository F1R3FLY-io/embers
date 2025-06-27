use firefly_client::{ReadNodeClient, template};

use crate::ai_agents::blockchain::dtos;
use crate::ai_agents::models::Agents;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;

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

    let agents: Option<Vec<dtos::AgentHeader>> = client.get_data(code).await?;
    Ok(agents.map(|mut agents| {
        agents.sort_by(|l, r| l.version.cmp(&r.version));
        Agents {
            agents: agents.into_iter().map(Into::into).collect(),
        }
    }))
}
