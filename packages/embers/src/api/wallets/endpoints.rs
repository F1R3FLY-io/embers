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
use futures::future;
use futures::sink::SinkExt;
use poem::web::{Data, websocket};
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;
use poem_openapi::types::ToJSON;

use crate::api::common::{
    ApiTags,
    PrepareResponse,
    SendRequest,
    SendResp,
    SignedContract,
    Stringified,
};
use crate::api::wallets::models::{
    BoostReq,
    BoostResp,
    DeployEvent,
    TransferReq,
    TransferResp,
    WalletStateAndHistory,
};
use crate::domain::wallets::WalletsService;

pub struct WalletsApi<R = ReadNodeClient, W = WriteNodeClient, N = NodeEvents>(
    PhantomData<(R, W, N)>,
);

impl<R, W, N> Default for WalletsApi<R, W, N> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[OpenApi(prefix_path = "/wallets", tag = ApiTags::Wallets)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> WalletsApi<R, W, N> {
    #[oai(
        path = "/:address/state",
        method = "get",
        operation_id = "wallets_state_and_history"
    )]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
    ) -> poem::Result<Json<WalletStateAndHistory>> {
        let wallet_state_and_history = wallets
            .get_wallet_state_and_history(address.0)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[oai(
        path = "/transfer/prepare",
        method = "post",
        operation_id = "wallets_prepare_transfer"
    )]
    async fn prepare_transfer(
        &self,
        Json(body): Json<TransferReq>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<TransferResp>>> {
        PrepareResponse::from_call(
            body,
            |body| wallets.prepare_transfer_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(
        path = "/transfer/send",
        method = "post",
        operation_id = "wallets_send_transfer"
    )]
    async fn transfer(
        &self,
        SendRequest(body): SendRequest<SignedContract, TransferReq, TransferResp>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = wallets.deploy_signed_transfer(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(
        path = "/boost/prepare",
        method = "post",
        operation_id = "wallets_prepare_boost"
    )]
    async fn prepare_boost(
        &self,
        Json(body): Json<BoostReq>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<BoostResp>>> {
        PrepareResponse::from_call(
            body,
            |body| wallets.prepare_boost_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(
        path = "/boost/send",
        method = "post",
        operation_id = "wallets_send_boost"
    )]
    async fn boost(
        &self,
        SendRequest(body): SendRequest<SignedContract, BoostReq, BoostResp>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = wallets.deploy_boost_transfer(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[allow(clippy::unused_async)]
    #[oai(
        path = "/:address/deploys",
        method = "get",
        operation_id = "wallets_subscribe_deploys"
    )]
    async fn deploys(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(wallets): Data<&WalletsService<R, W, N>>,
        ws: websocket::WebSocket,
    ) -> websocket::BoxWebSocketUpgraded {
        let wallets = wallets.clone();

        ws.on_upgrade(move |socket| {
            let sink = socket.with(|msg| {
                let msg = DeployEvent::from(msg).to_json_string();
                future::ok(websocket::Message::Text(msg))
            });
            wallets.subscribe_to_deploys(address.0, sink)
        })
        .boxed()
    }
}
