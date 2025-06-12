use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use uuid::Uuid;

use crate::ai_agents::models::{CreateAgentReq, CreateAgentResp, Directory};
use crate::common::deploy_signed_contract;
use crate::common::rendering::{PrepareForSigning, RhoValue};
use crate::common::tracing::record_trace;

#[derive(Template)]
#[template(path = "ai_agents/create_agent.rho", escape = "none")]
struct CreateAgent {
    id: RhoValue<String>,
    version: RhoValue<String>,
    name: RhoValue<String>,
    shard: RhoValue<Option<String>>,
    filesystem: RhoValue<Option<Directory>>,
}

#[tracing::instrument(level = "info", skip_all, fields(request), ret(Debug, level = "trace"))]
pub fn prepare_create_agent_contract(request: CreateAgentReq) -> CreateAgentResp {
    record_trace!(request);

    let id = Uuid::new_v4();
    let version = Uuid::now_v7();

    let contract = CreateAgent {
        id: id.to_string().into(),
        version: version.to_string().into(),
        name: request.name.into(),
        shard: request.shard.into(),
        filesystem: request.filesystem.into(),
    }
    .prepare_for_signing();

    CreateAgentResp {
        id: id.into(),
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
pub async fn deploy_signed_create_agent(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await
}
