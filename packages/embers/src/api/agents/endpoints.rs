use firefly_client::models::WalletAddress;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::api::agents::models::{
    Agent,
    Agents,
    CreateAgentReq,
    CreateAgentResp,
    DeleteAgentResp,
    DeployAgentReq,
    DeployAgentResp,
    DeploySignedAgentReq,
    SaveAgentReq,
    SaveAgentResp,
};
use crate::api::common::{
    ApiTags,
    MaybeNotFound,
    PrepareResponse,
    SendRequest,
    SendResp,
    SignedContract,
    Stringified,
};
use crate::domain::agents::AgentsService;

#[derive(Debug, Clone)]
pub struct AgentsApi;

#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl AgentsApi {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<Agents>> {
        let agents = agents.list(address.0).await?;
        Ok(Json(agents.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> MaybeNotFound<Agents> {
        agents.list_versions(address.0, id).await.into()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> MaybeNotFound<Agent> {
        agents.get(address.0, id, version).await.into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateAgentReq>,
        Data(agents): Data<&AgentsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<CreateAgentResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents.prepare_create_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateAgentReq, CreateAgentResp>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_create(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployAgentReq>,
        Data(agents): Data<&AgentsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<DeployAgentResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents.prepare_deploy_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy(
        &self,
        SendRequest(body): SendRequest<DeploySignedAgentReq, DeployAgentReq, DeployAgentResp>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_deploy(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveAgentReq>,
        Data(agents): Data<&AgentsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<SaveAgentResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents.prepare_save_contract(id, body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveAgentReq, SaveAgentResp>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_save(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:id/delete/prepare", method = "post")]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<DeleteAgentResp>> {
        let contract = agents.prepare_delete_contract(id).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/delete/send", method = "post")]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_delete(body.into()).await?;
        Ok(Json(deploy_id.into()))
    }
}
