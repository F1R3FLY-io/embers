use firefly_client::models::SignedCode;
use firefly_client::{WriteNodeClient, template};
use uuid::Uuid;

use crate::ai_agents::models::{SaveAgentReq, SaveAgentResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

template! {
    #[template(path = "ai_agents/save_agent.rho")]
    #[derive(Debug, Clone)]
    struct SaveAgent {
        id: String,
        version: Uuid,
        name: String,
        shard: Option<String>,
        code: Option<String>,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(id, request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub fn prepare_save_agent_contract(
    id: String,
    request: SaveAgentReq,
) -> anyhow::Result<SaveAgentResp> {
    record_trace!(id, request);

    let version = Uuid::now_v7();

    let contract = SaveAgent {
        id,
        version,
        name: request.name,
        shard: request.shard,
        code: request.code,
    }
    .render()?;

    Ok(SaveAgentResp {
        version: version.into(),
        contract: prepare_for_signing(contract),
    })
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_save_agent(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await?;
    Ok(())
}
