use firefly_client::models::SignedCode;
use firefly_client::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::common::dtos::{ApiTags, ParseFromString, PreparedContractDto, SignedContractDto};
use crate::wallets::dtos::{PrepareTransferInputDto, WalletStateAndHistoryDto};
use crate::wallets::handlers::{
    deploy_signed_transfer,
    get_wallet_state_and_history,
    prepare_transfer_contract,
};
use crate::wallets::models::{PrepareTransferInput, WalletAddress};

pub struct WalletsApi;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/wallet", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Data(read_client): Data<&ReadNodeClient>,
        Data(block_client): Data<&BlocksClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_state_and_history =
            get_wallet_state_and_history(read_client, block_client, address.0).await?;

        Ok(Json(wallet_state_and_history.into()))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContractDto>> {
        let value = PrepareTransferInput::try_from(input)?;
        let contract = prepare_transfer_contract(value);

        Ok(Json(contract.into()))
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
