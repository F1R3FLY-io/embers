use firefly_client::WriteNodeClient;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents::dtos::{
    Agent, Agents, CreateAgentReq, CreateAgentResp, DeployAgentReq, DeployAgentResp, SaveAgentReq,
    SaveAgentResp, TestAgentReq, TestAgentResp,
};
use crate::ai_agents::handlers::{deploy_signed_create_agent, prepare_create_agent_contract};
use crate::common::dtos::{ApiTags, SignedContractDto};

pub struct AIAgents;

#[allow(unused_variables, clippy::unused_async)]
#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl AIAgents {
    #[oai(path = "/", method = "get")]
    async fn list(&self) -> poem::Result<Json<Agents>> {
        todo!()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(input): Json<CreateAgentReq>,
    ) -> poem::Result<Json<CreateAgentResp>> {
        let contract = prepare_create_agent_contract(input.into());
        Ok(Json(contract.into()))
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        Data(client): Data<&WriteNodeClient>,
        Json(body): Json<SignedContractDto>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_create_agent(&mut client, body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/test", method = "post")]
    async fn test(&self, Json(input): Json<TestAgentReq>) -> poem::Result<Json<TestAgentResp>> {
        todo!()
    }

    #[oai(path = "/:id/versions", method = "get")]
    async fn list_versions(&self, Path(id): Path<String>) -> poem::Result<Json<Agents>> {
        todo!()
    }

    #[oai(path = "/:id/save", method = "post")]
    async fn save(
        &self,
        Path(id): Path<String>,
        Json(input): Json<SaveAgentReq>,
    ) -> poem::Result<Json<SaveAgentResp>> {
        todo!()
    }

    #[oai(path = "/:id/:version", method = "get")]
    async fn get(
        &self,
        Path(id): Path<String>,
        Path(version): Path<String>,
    ) -> poem::Result<Json<Agent>> {
        todo!()
    }

    #[oai(path = "/:id/:version/deploy", method = "post")]
    async fn deploy(
        &self,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Json(input): Json<DeployAgentReq>,
    ) -> poem::Result<Json<DeployAgentResp>> {
        todo!()
    }
}
