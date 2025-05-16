use poem::web::Data;
use poem_openapi::payload::Json;
use poem_openapi::{Object, OpenApi};

use super::Tag;
use super::models::{RevAddress, Stringified, WalletStateAndHistory};
use crate::domain::wallet::WalletService;

#[derive(Debug, Clone, Object)]
pub struct TransferResponse {
    cost: Stringified<u64>,
}

#[derive(Debug, Clone, Object)]
pub struct TransferRequest {
    amount: Stringified<u64>,
    to_address: RevAddress,
    description: Option<String>,
}

pub struct WalletApi;

#[OpenApi(prefix_path = "/wallet", tag = Tag::Wallet)]
impl WalletApi {
    #[oai(path = "/state", method = "get")]
    async fn wallet_state_and_history(
        &self,
        wallet: Data<&WalletService>,
    ) -> poem::Result<Json<WalletStateAndHistory>> {
        let state = wallet.get_state_and_history().await?;
        Ok(Json(state.into()))
    }

    #[oai(path = "/transfer", method = "post")]
    async fn transfer(
        &self,
        wallet: Data<&WalletService>,
        Json(body): Json<TransferRequest>,
    ) -> poem::Result<Json<TransferResponse>> {
        let cost = wallet
            .to_owned()
            .transfer(body.amount.0, body.to_address.0, body.description)
            .await?;

        Ok(Json(TransferResponse {
            cost: cost.0.into(),
        }))
    }
}
