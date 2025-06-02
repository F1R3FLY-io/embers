use firefly_client::models::SignedCode;
use firefly_client::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::common::dtos::{ApiTags, PreparedContractDto, SignedContractDto};
use crate::wallets::dtos::{PrepareTransferInputDto, WalletStateAndHistoryDto};
use crate::wallets::handlers::{
    deploy_signed_transfer,
    get_wallet_state_and_history,
    prepare_transfer_contract,
};
use crate::wallets::models::{PrepareTransferInput, WalletAddress};
use super::dtos::{PreparedContractDto, WalletStateAndHistoryDto};
use crate::FireFlyClients;
use crate::wallet::contracts::{PrepareTransferInput, prepare_contract};
use crate::wallet::dtos::{PrepareTransferInputDto, TransferSendDto};
use crate::wallet::handlers::{deploy_signed_contract, get_wallet_state_and_history};
use crate::wallet::models::WalletAddress;

pub struct WalletsApi;

#[OpenApi(prefix_path = "/wallet", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
        Data(block_client): Data<&BlocksClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_address =
            WalletAddress::try_from(address).map_err(|_| poem::error::ParsePathError)?;

        let wallet_state_and_history = get_wallet_state_and_history(&client.0, wallet_address)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[allow(clippy::unused_async)]
    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContractDto>> {
        let id_generator = uuid::Uuid::now_v7;
        let input = PrepareTransferInput::try_from(input)?;

        let result = prepare_contract(id_generator, input);

        Ok(Json(result.into()))
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<SignedContractDto>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let (_, mut write_node_client, _) = client.to_owned();

        let code = SignedCode::from(body);

        deploy_signed_contract(&mut write_node_client, code)
            .await
            .map_err(Into::into)
    }
}
