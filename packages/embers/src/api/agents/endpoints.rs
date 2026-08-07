use core::marker::PhantomData;

use firefly_client::models::WalletAddress;
use firefly_client::{
    NodeEventSource,
    NodeEvents,
    ReadNode,
    ReadNodeClient,
    WriteNode,
    WriteNodeClient,
};
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

pub struct AgentsApi<R = ReadNodeClient, W = WriteNodeClient, N = NodeEvents>(
    PhantomData<(R, W, N)>,
);

impl<R, W, N> Default for AgentsApi<R, W, N> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[OpenApi(prefix_path = "/ai-agents", tag = ApiTags::AIAgents)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsApi<R, W, N> {
    #[oai(path = "/:address", method = "get", operation_id = "agents_list")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<Agents>> {
        let agents = agents.list(address.0).await?;
        Ok(Json(agents.into()))
    }

    #[oai(
        path = "/:address/:id/versions",
        method = "get",
        operation_id = "agents_list_versions"
    )]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> MaybeNotFound<Agents> {
        agents.list_versions(address.0, id).await.into()
    }

    #[oai(
        path = "/:address/:id/versions/:version",
        method = "get",
        operation_id = "agents_get_version"
    )]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> MaybeNotFound<Agent> {
        agents.get(address.0, id, version).await.into()
    }

    #[oai(
        path = "/create/prepare",
        method = "post",
        operation_id = "agents_prepare_create"
    )]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateAgentReq>,
        Data(agents): Data<&AgentsService<R, W, N>>,
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

    #[oai(
        path = "/create/send",
        method = "post",
        operation_id = "agents_send_create"
    )]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateAgentReq, CreateAgentResp>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_create(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(
        path = "/deploy/prepare",
        method = "post",
        operation_id = "agents_prepare_deploy"
    )]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployAgentReq>,
        Data(agents): Data<&AgentsService<R, W, N>>,
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

    #[oai(
        path = "/deploy/send",
        method = "post",
        operation_id = "agents_send_deploy"
    )]
    async fn deploy(
        &self,
        SendRequest(body): SendRequest<DeploySignedAgentReq, DeployAgentReq, DeployAgentResp>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_deploy(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(
        path = "/:id/save/prepare",
        method = "post",
        operation_id = "agents_prepare_save"
    )]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveAgentReq>,
        Data(agents): Data<&AgentsService<R, W, N>>,
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

    #[oai(
        path = "/:id/save/send",
        method = "post",
        operation_id = "agents_send_save"
    )]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveAgentReq, SaveAgentResp>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_save(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(
        path = "/:id/delete/prepare",
        method = "post",
        operation_id = "agents_prepare_delete"
    )]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<DeleteAgentResp>> {
        let contract = agents.prepare_delete_contract(id).await?;
        Ok(Json(contract.into()))
    }

    #[oai(
        path = "/:id/delete/send",
        method = "post",
        operation_id = "agents_send_delete"
    )]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_delete(body.into()).await?;
        Ok(Json(deploy_id.into()))
    }
}
