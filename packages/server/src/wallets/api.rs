use firefly_client::models::SignedCode;
use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use super::handlers::{deploy_signed_transfer, prepare_transfer_contract};
use crate::common::api::dtos::{ApiTags, ParseFromString, SignedContract};
use crate::common::models::WalletAddress;
use crate::wallets::api::dtos::{TransferReq, TransferResp, WalletStateAndHistory};
use crate::wallets::handlers::get_wallet_state_and_history;

mod dtos;

#[derive(Debug, Clone)]
pub struct WalletsApi;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/wallets", tag = ApiTags::Wallets)]
impl WalletsApi {
    #[oai(path = "/:address/state", method = "get")]
    async fn wallet_state_and_history(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<WalletStateAndHistory>> {
        let wallet_state_and_history = get_wallet_state_and_history(read_client, address.0)
            .await
            .map(Into::into)?;

        Ok(Json(wallet_state_and_history))
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<TransferReq>,
    ) -> poem::Result<Json<TransferResp>> {
        let input = input.try_into()?;
        let result = prepare_transfer_contract(input)?;

        Ok(Json(TransferResp {
            contract: result.into(),
        }))
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        let code = SignedCode::from(body);

        deploy_signed_transfer(&mut client, code)
            .await
            .map_err(Into::into)
    }
}
