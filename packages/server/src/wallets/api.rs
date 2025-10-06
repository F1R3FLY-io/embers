use firefly_client::models::SignedCode;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::common::api::dtos::{ApiTags, SignedContract, Stringified};
use crate::common::models::WalletAddress;
use crate::wallets::api::dtos::{TransferReq, TransferResp, WalletStateAndHistory};
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
        Json(input): Json<TransferReq>,
        Data(wallets): Data<&WalletsService>,
    ) -> poem::Result<Json<TransferResp>> {
        let input = input.try_into()?;
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
    ) -> poem::Result<()> {
        let code = SignedCode::from(body);

        wallets
            .deploy_signed_transfer(code)
            .await
            .map_err(Into::into)
    }
}
