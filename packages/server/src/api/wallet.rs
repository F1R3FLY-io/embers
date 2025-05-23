use etc::SignedContract;
use firefly_client::FireflyClient;
use poem::{http::StatusCode, web::Data};
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;
use wallet::{deploy_signed_contract, get_wallet_state_and_history, prepare_contract};

use super::Tag;
use crate::dtos::{
    prepare_transfer::PrepareTransferInputDto, prepared_contract::PreparedContractDto,
    transfer_send::TransferSendDto, wallet_state_and_history::WalletStateAndHistoryDto,
};

pub struct WalletApi;

#[OpenApi(prefix_path = "/wallet", tag = Tag::Wallet)]
impl WalletApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        wallet_address: String,
        client: Data<&FireflyClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        wallet_address
            .try_into()
            .map_err(|_| poem::error::ParsePathError)
            .map(|address| get_wallet_state_and_history(&client.0, address))?
            .await
            .map_err(|e| poem::Error::from_string(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR))
            .map(Into::into)
            .map(Json)
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContractDto>> {
        input
            .try_into()
            .map(prepare_contract)
            .map(Into::into)
            .map(Json)
            .map_err(Into::into)
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<TransferSendDto>,
        client: Data<&FireflyClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();

        SignedContract::try_from(body)
            .map(|contract| deploy_signed_contract(&mut client, contract))?
            .await
            .map_err(Into::into)
    }
}
