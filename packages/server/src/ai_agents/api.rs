use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents::models::{
    Agent,
    Agents,
    CreateAgentReq,
    CreateAgentResp,
    DeployAgentReq,
    DeployAgentResp,
    SaveAgentReq,
    SaveAgentResp,
    TestAgentReq,
    TestAgentResp,
};
use crate::common::dtos::ApiTags;

pub struct AIAgents;

#[allow(unused_variables, clippy::unused_async)]
#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl AIAgents {
    #[oai(path = "/", method = "get")]
    async fn list(&self) -> poem::Result<Json<Agents>> {
        todo!()
    }

    #[oai(path = "/create", method = "post")]
    async fn create(
        &self,
        Json(input): Json<CreateAgentReq>,
    ) -> poem::Result<Json<CreateAgentResp>> {
        todo!()
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
