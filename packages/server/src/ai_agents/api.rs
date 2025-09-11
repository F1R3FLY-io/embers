use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents::api::dtos::{
    Agent,
    Agents,
    CreateAgentReq,
    CreateAgentResp,
    DeployAgentResp,
    SaveAgentReq,
    SaveAgentResp,
};
use crate::ai_agents::handlers::{
    deploy_signed_create_agent,
    deploy_signed_deploy_agent,
    deploy_signed_save_agent,
    get_agent,
    list_agent_versions,
    list_agents,
    prepare_create_agent_contract,
    prepare_deploy_agent_contract,
    prepare_save_agent_contract,
};
use crate::common::api::dtos::{ApiTags, MaybeNotFound, ParseFromString, SignedContract};
use crate::common::models::WalletAddress;

mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgents;

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

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
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
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<Json<CreateAgentResp>> {
        let mut client = client.to_owned();
        let contract = prepare_create_agent_contract(input.into(), &mut client).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_create_agent(&mut client, body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(input): Json<SaveAgentReq>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<Json<SaveAgentResp>> {
        let mut client = client.to_owned();
        let contract = prepare_save_agent_contract(id, input.into(), &mut client).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_save_agent(&mut client, body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(
        path = "/:address/:id/versions/:version/deploy/prepare",
        method = "post"
    )]
    async fn prepare_deploy_agent(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(client): Data<&WriteNodeClient>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<DeployAgentResp>> {
        let mut client = client.to_owned();
        let contract =
            prepare_deploy_agent_contract(address.0, id, version, &mut client, read_client).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:address/:id/versions/:version/deploy/send", method = "post")]
    async fn deploy_agent(
        &self,
        #[allow(unused_variables)] Path(address): Path<ParseFromString<WalletAddress>>,
        #[allow(unused_variables)] Path(id): Path<String>,
        #[allow(unused_variables)] Path(version): Path<String>,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_deploy_agent(&mut client, body.into())
            .await
            .map_err(Into::into)
    }
}
