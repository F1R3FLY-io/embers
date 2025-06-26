use anyhow::Context;
use firefly_client::models::SignedCode;
use firefly_client::{ReadNodeClient, WriteNodeClient};

use crate::ai_agents::handlers::get_agent;
use crate::ai_agents::models::DeployAgentResp;
use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address, id, version),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn prepare_deploy_agent_contract(
    address: WalletAddress,
    id: String,
    version: String,
    client: &ReadNodeClient,
) -> anyhow::Result<DeployAgentResp> {
    record_trace!(address, id, version);

    let agent = get_agent(address, id, version, client)
        .await?
        .context("agent not found")?;

    let code = agent.code.context("agent has no code")?;

    Ok(DeployAgentResp {
        contract: prepare_for_signing(code),
    })
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_deploy_agent(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await
}
