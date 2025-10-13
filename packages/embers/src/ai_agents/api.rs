use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents::api::dtos::{
    Agent,
    Agents,
    CreateAgentReq,
    CreateAgentResp,
    DeleteAgentResp,
    DeployAgentReq,
    DeployAgentResp,
    SaveAgentReq,
    SaveAgentResp,
};
use crate::ai_agents::handlers::AgentsService;
use crate::common::api::dtos::{ApiTags, MaybeNotFound, SignedContract, Stringified};
use crate::common::models::WalletAddress;

mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgents;

#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl AIAgents {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<Agents>> {
        let agents = agents.list_agents(address.0).await?;
        Ok(Json(agents.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> MaybeNotFound<Agents> {
        agents.list_agent_versions(address.0, id).await.into()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> MaybeNotFound<Agent> {
        agents.get_agent(address.0, id, version).await.into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateAgentReq>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<CreateAgentResp>> {
        let contract = agents.prepare_create_agent_contract(body.into()).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<()> {
        agents
            .deploy_signed_create_agent(body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy_agent(
        &self,
        Json(body): Json<DeployAgentReq>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<DeployAgentResp>> {
        let contract = agents.prepare_deploy_agent_contract(body.into()).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy_agent(
        &self,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<()> {
        agents
            .deploy_signed_deploy_agent(body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveAgentReq>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<SaveAgentResp>> {
        let contract = agents.prepare_save_agent_contract(id, body.into()).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<()> {
        agents
            .deploy_signed_save_agent(body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/:id/delete/prepare", method = "post")]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<DeleteAgentResp>> {
        let contract = agents.prepare_delete_agent_contract(id).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/delete/send", method = "post")]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<()> {
        agents
            .deploy_signed_delete_agent(body.into())
            .await
            .map_err(Into::into)
    }
}
