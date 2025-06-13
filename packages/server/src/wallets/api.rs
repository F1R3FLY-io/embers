use firefly_client::models::SignedCode;
use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::common::dtos::{ApiTags, SignedContractDto};
use crate::common::models::PreparedContract;
use crate::wallets::dtos::{PrepareTransferInputDto, WalletStateAndHistoryDto};
use crate::wallets::handlers::get_wallet_state_and_history;
use crate::wallets::models::{PrepareTransferInput, WalletAddress};

use super::handlers::{deploy_signed_transfer, prepare_transfer_contract};

pub struct WalletsApi;

#[OpenApi(prefix_path = "/wallet", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_address =
            WalletAddress::try_from(address).map_err(|_| poem::error::ParsePathError)?;

        let wallet_state_and_history = get_wallet_state_and_history(read_client, wallet_address)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[allow(clippy::unused_async)]
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
