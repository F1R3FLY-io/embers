use firefly_client::models::WalletAddress;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::api::common::{
    ApiTags,
    MaybeNotFound,
    PrepareResponse,
    SendRequest,
    SendResp,
    SignedContract,
    Stringified,
};
use crate::api::oslfs::models::{
    CreateOslfReq,
    CreateOslfResp,
    DeleteOslfResp,
    Oslf,
    Oslfs,
    SaveOslfReq,
    SaveOslfResp,
};
use crate::domain::oslfs::OslfsService;

#[derive(Debug, Clone)]
pub struct OslfsApi;

#[OpenApi(prefix_path = "/oslfs", tag = ApiTags::Oslfs)]
impl OslfsApi {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(oslfs): Data<&OslfsService>,
    ) -> poem::Result<Json<Oslfs>> {
        let oslfs = oslfs.list(address.0).await?;
        Ok(Json(oslfs.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(oslfs): Data<&OslfsService>,
    ) -> MaybeNotFound<Oslfs> {
        oslfs.list_versions(address.0, id).await.into()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(oslfs): Data<&OslfsService>,
    ) -> MaybeNotFound<Oslf> {
        oslfs.get(address.0, id, version).await.into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateOslfReq>,
        Data(oslfs): Data<&OslfsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<CreateOslfResp>>> {
        PrepareResponse::from_call(
            body,
            |body| oslfs.prepare_create_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateOslfReq, CreateOslfResp>,
        Data(oslfs): Data<&OslfsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = oslfs.deploy_signed_create(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveOslfReq>,
        Data(oslfs): Data<&OslfsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<SaveOslfResp>>> {
        PrepareResponse::from_call(
            body,
            |body| oslfs.prepare_save_contract(id, body.into()),
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
        SendRequest(body): SendRequest<SignedContract, SaveOslfReq, SaveOslfResp>,
        Data(oslfs): Data<&OslfsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = oslfs.deploy_signed_save(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:id/delete/prepare", method = "post")]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(oslfs): Data<&OslfsService>,
    ) -> poem::Result<Json<DeleteOslfResp>> {
        let contract = oslfs.prepare_delete_contract(id).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/delete/send", method = "post")]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(oslfs): Data<&OslfsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = oslfs.deploy_signed_delete(body.into()).await?;
        Ok(Json(deploy_id.into()))
    }
}
