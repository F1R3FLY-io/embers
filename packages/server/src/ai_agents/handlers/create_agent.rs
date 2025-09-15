use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use firefly_client::rendering::Render;
use uuid::Uuid;

use crate::ai_agents::models::{CreateAgentReq, CreateAgentResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/create_agent.rho")]
struct CreateAgent {
    id: Uuid,
    version: Uuid,
    name: String,
    shard: Option<String>,
    code: Option<String>,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn prepare_create_agent_contract(
    request: CreateAgentReq,
    client: &mut WriteNodeClient,
) -> anyhow::Result<CreateAgentResp> {
    record_trace!(request);

    let id = Uuid::new_v4();
    let version = Uuid::now_v7();

    let contract = CreateAgent {
        id,
        version,
        name: request.name,
        shard: request.shard,
        code: request.code,
    }
    .render()?;

    let valid_after = client.get_head_block_index().await?;
    Ok(CreateAgentResp {
        id: id.into(),
        version: version.into(),
        contract: prepare_for_signing()
            .code(contract)
            .valid_after_block_number(valid_after)
            .call(),
    })
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_create_agent(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await?;
    Ok(())
}
