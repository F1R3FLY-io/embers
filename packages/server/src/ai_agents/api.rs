use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents::dtos::{
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
use crate::ai_agents::handlers::{
    deploy_signed_create_agent,
    deploy_signed_save_agent,
    get_agent,
    list_agent_versions,
    list_agents,
    prepare_create_agent_contract,
    prepare_save_agent_contract,
};
use crate::common::dtos::{ApiTags, MaybeNotFound, ParseFromString, SignedContractDto};
use crate::wallets::models::WalletAddress;

#[derive(Debug, Clone)]
pub struct AIAgents;

#[allow(unused_variables, clippy::unused_async)]
#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl AIAgents {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<Agents>> {
        let agents = list_agents(address.0, read_client).await?;
        Ok(Json(agents.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Path(id): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> MaybeNotFound<Agents> {
        list_agent_versions(address.0, id, read_client).await.into()
    }

    #[oai(path = "/:address/:id/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> MaybeNotFound<Agent> {
        get_agent(address.0, id, version, read_client).await.into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(input): Json<CreateAgentReq>,
    ) -> poem::Result<Json<CreateAgentResp>> {
        let contract = prepare_create_agent_contract(input.into())?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        Json(body): Json<SignedContractDto>,
        Data(client): Data<&WriteNodeClient>,
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

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(input): Json<SaveAgentReq>,
    ) -> poem::Result<Json<SaveAgentResp>> {
        let contract = prepare_save_agent_contract(id, input.into())?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        Json(body): Json<SignedContractDto>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_save_agent(&mut client, body.into())
            .await
            .map_err(Into::into)
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
