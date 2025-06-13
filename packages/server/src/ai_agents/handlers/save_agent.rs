use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use uuid::Uuid;

use crate::ai_agents::models::{Directory, SaveAgentReq, SaveAgentResp};
use crate::common::deploy_signed_contract;
use crate::common::rendering::{PrepareForSigning, RhoValue};
use crate::common::tracing::record_trace;

#[derive(Template)]
#[template(path = "ai_agents/save_agent.rho", escape = "none")]
struct SaveAgent {
    id: RhoValue<String>,
    version: RhoValue<String>,
    name: RhoValue<String>,
    shard: RhoValue<Option<String>>,
    filesystem: RhoValue<Option<Directory>>,
}

#[tracing::instrument(level = "info", skip_all, fields(request), ret(Debug, level = "trace"))]
pub fn prepare_save_agent_contract(id: String, request: SaveAgentReq) -> SaveAgentResp {
    record_trace!(request);

    let version = Uuid::now_v7();

    let contract = SaveAgent {
        id: id.into(),
        version: version.to_string().into(),
        name: request.name.into(),
        shard: request.shard.into(),
        filesystem: request.filesystem.into(),
    }
    .prepare_for_signing();

    SaveAgentResp {
        version: version.into(),
        contract,
    }
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

    deploy_signed_contract(client, contract).await
}
