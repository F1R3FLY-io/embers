use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use uuid::Uuid;

use crate::ai_agents::models::{CreateAgentReq, CreateAgentResp, Directory};
use crate::common::deploy_signed_contract;
use crate::common::rendering::{PrepareForSigning, RhoValue};

#[derive(Template)]
#[template(path = "ai_agents/create_agent.rho", escape = "none")]
pub struct CreateAgent {
    pub id: RhoValue<String>,
    pub version: RhoValue<String>,
    pub name: RhoValue<String>,
    pub shard: RhoValue<Option<String>>,
    pub filesystem: RhoValue<Option<Directory>>,
}

#[tracing::instrument(level = "info", skip_all)]
#[tracing::instrument(level = "trace", ret(Debug))]
pub fn prepare_create_agent_contract(value: CreateAgentReq) -> CreateAgentResp {
    let id = Uuid::now_v7();
    let version = Uuid::now_v7();

    let contract = CreateAgent {
        id: id.to_string().into(),
        version: version.to_string().into(),
        name: value.name.into(),
        shard: value.shard.into(),
        filesystem: value.filesystem.into(),
    }
    .prepare_for_signing();

    CreateAgentResp {
        id: id.into(),
        version: version.into(),
        contract,
    }
}

#[tracing::instrument(level = "info", skip_all, err(Debug))]
#[tracing::instrument(level = "trace", skip(client), ret(Debug))]
pub async fn deploy_signed_create_agent(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    deploy_signed_contract(client, contract).await
}
