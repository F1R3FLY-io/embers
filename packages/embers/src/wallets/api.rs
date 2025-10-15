use firefly_client::models::WalletAddress;
use futures::future;
use futures::sink::SinkExt;
use poem::web::{Data, websocket};
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;
use poem_openapi::types::ToJSON;

use crate::common::api::dtos::{ApiTags, SendResp, SignedContract, Stringified};
use crate::wallets::api::dtos::{TransferReq, TransferResp, WalletEvent, WalletStateAndHistory};
use crate::wallets::handlers::WalletsService;

mod dtos;

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
    ) -> poem::Result<Json<TransferResp>> {
        let input = body.try_into()?;
        let result = wallets.prepare_transfer_contract(input).await?;

        Ok(Json(TransferResp {
            contract: result.into(),
        }))
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<SignedContract>,
        Data(wallets): Data<&WalletsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = wallets.deploy_signed_transfer(body.into()).await?;
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
                let msg = WalletEvent::from(msg).to_json_string();
                future::ok(websocket::Message::Text(msg))
            });
            wallets.subscribe_to_deploys(address.0, sink);
            future::ready(())
        })
        .boxed()
    }
}
