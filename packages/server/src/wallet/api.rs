use firefly_client::models::SignedCode;
use firefly_client::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::wallet::dtos::{
    PrepareTransferInputDto,
    PreparedContractDto,
    TransferSendDto,
    WalletStateAndHistoryDto,
};
use crate::wallet::handlers::{
    PrepareTransferInput,
    deploy_signed_contract,
    get_wallet_state_and_history,
    prepare_transfer_contract,
};
use crate::wallet::models::WalletAddress;

pub struct WalletApi;

#[OpenApi(prefix_path = "/wallet")]
impl WalletApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
        Data(block_client): Data<&BlocksClient>,
    ) -> poem::Result<Json<WalletStateAndHistoryDto>> {
        let wallet_address =
            WalletAddress::try_from(address).map_err(|_| poem::error::ParsePathError)?;

        let wallet_state_and_history =
            get_wallet_state_and_history(read_client, block_client, wallet_address).await?;

        Ok(Json(wallet_state_and_history.into()))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    #[allow(clippy::unused_async)]
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
        Json(body): Json<TransferSendDto>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();

        let code = SignedCode::from(body);

        deploy_signed_contract(&mut client, code)
            .await
            .map_err(Into::into)
    }
}
