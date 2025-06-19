use firefly_client::models::SignedCode;
use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use super::handlers::{deploy_signed_transfer, prepare_transfer_contract};
use crate::common::dtos::{ApiTags, ParseFromString, SignedContractDto};
use crate::common::models::PreparedContract;
use crate::wallets::dtos::{PrepareTransferInputDto, WalletStateAndHistoryDto};
use crate::wallets::handlers::get_wallet_state_and_history;
use crate::wallets::models::{PrepareTransferInput, WalletAddress};

#[derive(Debug, Clone)]
pub struct WalletsApi;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/wallet", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(wallet_address): Path<ParseFromString<WalletAddress>>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_state_and_history = get_wallet_state_and_history(read_client, wallet_address.0)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContract>> {
        let input = PrepareTransferInput::try_from(input).map_err(anyhow::Error::from)?;
        let result = prepare_transfer_contract(input);

        poem::Result::Ok(Json(result))
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<SignedContractDto>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        let code = SignedCode::from(body);

        deploy_signed_transfer(&mut client, code)
            .await
            .map_err(Into::into)
    }
}
