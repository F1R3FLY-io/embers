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
use crate::api::oslf::models::{
    CreateOslfReq,
    CreateOslfResp,
    DeleteOslfResp,
    Oslf,
    Oslfs,
    SaveOslfReq,
    SaveOslfResp,
};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone)]
pub struct OSLF;

#[allow(unused, clippy::unused_async)]
#[OpenApi(prefix_path = "/oslf", tag = ApiTags::Oslf)]
impl OSLF {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
    ) -> poem::Result<Json<Oslfs>> {
        todo!()
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
    ) -> MaybeNotFound<Oslfs> {
        todo!()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
    ) -> MaybeNotFound<Oslf> {
        todo!()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateOslfReq>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<CreateOslfResp>>> {
        todo!()
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateOslfReq, CreateOslfResp>,
    ) -> poem::Result<Json<SendResp>> {
        todo!()
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveOslfReq>,
    ) -> poem::Result<Json<PrepareResponse<SaveOslfResp>>> {
        todo!()
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveOslfReq, SaveOslfResp>,
    ) -> poem::Result<Json<SendResp>> {
        todo!()
    }

    #[oai(path = "/:id/delete/prepare", method = "post")]
    async fn prepare_delete(&self, Path(id): Path<String>) -> poem::Result<Json<DeleteOslfResp>> {
        todo!()
    }

    #[oai(path = "/:id/delete/send", method = "post")]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
    ) -> poem::Result<Json<SendResp>> {
        todo!()
    }
}
