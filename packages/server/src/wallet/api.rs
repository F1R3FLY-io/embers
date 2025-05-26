use firefly_client::signed_code::SignedCode;
use poem::http::StatusCode;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::FireFlyClients;
use crate::wallet::dtos::PrepareTransferInputDto;
use crate::wallet::dtos::PreparedContractDto;
use crate::wallet::dtos::TransferSendDto;
use crate::wallet::dtos::WalletStateAndHistoryDto;
use crate::wallet::handlers::PrepareTransferInput;
use crate::wallet::handlers::deploy_signed_contract;
use crate::wallet::handlers::get_wallet_state_and_history;
use crate::wallet::handlers::prepare_contract;
use crate::wallet::models::WalletAddress;

pub struct WalletApi;

#[OpenApi(prefix_path = "/wallet")]
impl WalletApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        wallet_address: String,
        Data(client): Data<&FireFlyClients>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_address =
            WalletAddress::try_from(wallet_address).map_err(|_| poem::error::ParsePathError)?;

        let wallet_state_and_history =
            get_wallet_state_and_history(&client.0, &client.2, wallet_address)
                .await
                .map_err(|e| {
                    poem::Error::from_string(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
                })?;

        Ok(Json(WalletStateAndHistoryDto::from(
            wallet_state_and_history,
        )))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContractDto>> {
        let value = PrepareTransferInput::try_from(input)?;
        let contract = prepare_contract(value);

        Ok(Json(PreparedContractDto::from(contract)))
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<TransferSendDto>,
        Data(client): Data<&FireFlyClients>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();

        SignedCode::try_from(body)
            .map(|contract| deploy_signed_contract(&mut client.1, contract))?
            .await
            .map_err(Into::into)
    }
}
