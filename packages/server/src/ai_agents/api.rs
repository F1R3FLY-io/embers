use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;
use secp256k1::SecretKey;

use crate::ai_agents::api::deploy_signed_test_resp::DeploySignedTestResp;
use crate::ai_agents::api::dtos::{
    Agent,
    Agents,
    CreateAgentReq,
    CreateAgentResp,
    CreateTestwalletResp,
    DeployAgentResp,
    DeploySignedTestReq,
    DeployTestReq,
    DeployTestResp,
    SaveAgentReq,
    SaveAgentResp,
};
use crate::ai_agents::handlers::{
    create_test_wallet,
    deploy_signed_create_agent,
    deploy_signed_deploy_agent,
    deploy_signed_save_agent,
    deploy_test_contract,
    get_agent,
    list_agent_versions,
    list_agents,
    prepare_create_agent_contract,
    prepare_deploy_agent_contract,
    prepare_save_agent_contract,
    prepare_test_contract,
};
use crate::common::api::TestNet;
use crate::common::api::dtos::{ApiTags, MaybeNotFound, ParseFromString, SignedContract};
use crate::common::models::WalletAddress;

mod deploy_signed_test_resp;
mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgents;

#[allow(clippy::unused_async)]
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
    ) -> poem::Result<Json<CreateAgentResp>> {
        let contract = prepare_create_agent_contract(input.into())?;
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

    #[oai(path = "/test/wallet", method = "post")]
    async fn create_test_wallet(
        &self,
        Data(test_client): Data<&TestNet<WriteNodeClient>>,
        Data(test_service_key): Data<&TestNet<SecretKey>>,
    ) -> poem::Result<Json<CreateTestwalletResp>> {
        let mut test_client = test_client.0.clone();

        let test_client = create_test_wallet(&mut test_client, &test_service_key.0).await?;
        Ok(Json(test_client.into()))
    }

    #[oai(path = "/test/deploy/prepare", method = "post")]
    async fn prepare_test(&self, Json(input): Json<DeployTestReq>) -> Json<DeployTestResp> {
        let contracts = prepare_test_contract(input.into());
        Json(contracts.into())
    }

    #[oai(path = "/test/deploy/send", method = "post")]
    async fn test(
        &self,
        Json(input): Json<DeploySignedTestReq>,
        Data(test_client): Data<&TestNet<WriteNodeClient>>,
        Data(test_read_client): Data<&TestNet<ReadNodeClient>>,
    ) -> poem::Result<Json<DeploySignedTestResp>> {
        let mut test_client = test_client.0.clone();
        let result =
            deploy_test_contract(&mut test_client, &test_read_client.0, input.into()).await?;
        Ok(Json(result.into()))
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
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<DeployAgentResp>> {
        let contract = prepare_deploy_agent_contract(address.0, id, version, read_client).await?;
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
