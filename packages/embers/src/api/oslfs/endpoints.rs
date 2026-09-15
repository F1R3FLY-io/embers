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

use crate::api::common::{ApiResult, ApiTags, MaybeNotFound, PrepareResponse, SendRequest, SendResp, SignedContract, Stringified};
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

pub struct OslfsApi<R = ReadNodeClient, W = WriteNodeClient, N = NodeEvents>(
    PhantomData<(R, W, N)>,
);

impl<R, W, N> Default for OslfsApi<R, W, N> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[OpenApi(prefix_path = "/oslfs", tag = ApiTags::Oslfs)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> OslfsApi<R, W, N> {
    #[oai(path = "/:address", method = "get", operation_id = "oslfs_list")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> ApiResult<Oslfs> {
        oslfs.list(address.0).await.into()
    }

    #[oai(
        path = "/:address/:id/versions",
        method = "get",
        operation_id = "oslfs_list_versions"
    )]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> MaybeNotFound<Oslfs> {
        oslfs.list_versions(address.0, id).await.into()
    }

    #[oai(
        path = "/:address/:id/versions/:version",
        method = "get",
        operation_id = "oslfs_get_version"
    )]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> MaybeNotFound<Oslf> {
        oslfs.get(address.0, id, version).await.into()
    }

    #[oai(
        path = "/create/prepare",
        method = "post",
        operation_id = "oslfs_prepare_create"
    )]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateOslfReq>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<CreateOslfResp>> {
        PrepareResponse::from_call(
            body,
            |body| oslfs.prepare_create_contract(body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/create/send",
        method = "post",
        operation_id = "oslfs_send_create"
    )]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateOslfReq, CreateOslfResp>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        oslfs.deploy_signed_create(body.request.into()).await.into()
    }

    #[oai(
        path = "/:id/save/prepare",
        method = "post",
        operation_id = "oslfs_prepare_save"
    )]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveOslfReq>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<SaveOslfResp>> {
        PrepareResponse::from_call(
            body,
            |body| oslfs.prepare_save_contract(id, body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/:id/save/send",
        method = "post",
        operation_id = "oslfs_send_save"
    )]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveOslfReq, SaveOslfResp>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        oslfs.deploy_signed_save(body.request.into()).await.into()
    }

    #[oai(
        path = "/:id/delete/prepare",
        method = "post",
        operation_id = "oslfs_prepare_delete"
    )]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> ApiResult<DeleteOslfResp> {
        oslfs.prepare_delete_contract(id).await.into()
    }

    #[oai(
        path = "/:id/delete/send",
        method = "post",
        operation_id = "oslfs_send_delete"
    )]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(oslfs): Data<&OslfsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        oslfs.deploy_signed_delete(body.into()).await.into()
    }
}
