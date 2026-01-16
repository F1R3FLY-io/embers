use firefly_client::models::WalletAddress;
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

#[derive(Debug, Clone)]
pub struct WalletsApi;

#[OpenApi(prefix_path = "/wallets", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/:address/state", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(wallets): Data<&WalletsService>,
    ) -> poem::Result<Json<WalletStateAndHistory>> {
        let wallet_state_and_history = wallets
            .get_wallet_state_and_history(address.0)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(body): Json<TransferReq>,
        Data(wallets): Data<&WalletsService>,
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

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        SendRequest(body): SendRequest<SignedContract, TransferReq, TransferResp>,
        Data(wallets): Data<&WalletsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = wallets.deploy_signed_transfer(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/boost/prepare", method = "post")]
    async fn prepare_boost(
        &self,
        Json(body): Json<BoostReq>,
        Data(wallets): Data<&WalletsService>,
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

    #[oai(path = "/boost/send", method = "post")]
    async fn boost(
        &self,
        SendRequest(body): SendRequest<SignedContract, BoostReq, BoostResp>,
        Data(wallets): Data<&WalletsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = wallets.deploy_boost_transfer(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[allow(clippy::unused_async)]
    #[oai(path = "/:address/deploys", method = "get")]
    async fn deploys(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(wallets): Data<&WalletsService>,
        ws: websocket::WebSocket,
    ) -> websocket::BoxWebSocketUpgraded {
        let wallets = wallets.clone();

        ws.on_upgrade(move |socket| {
            let sink = socket.with(|msg| {
                let msg = DeployEvent::from(msg).to_json_string();
                future::ok(websocket::Message::Text(msg))
            });
            wallets.subscribe_to_deploys(address.0, sink);
            future::ready(())
        })
        .boxed()
    }
}
