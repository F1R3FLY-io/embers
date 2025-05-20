use firefly_client::FireflyClient;
use poem::http::StatusCode;
use poem::web::Data;
use poem_openapi::payload::Json;
use poem_openapi::{Object, OpenApi};

use super::Tag;
use crate::domain::models::WalletStateAndHistory;
use crate::domain::wallet::WalletService;
use crate::handlers::prepare_contract::{
    self, PrepareContractRequestProblem, PrepareTransferInputDto, PreparedContract,
};

#[derive(Debug, Clone, Object)]
pub struct TransferResponse {
    cost: String,
}

#[derive(Debug, Clone, Object)]
pub struct TransferSendDto {
    transfer_signed_contract: Vec<u8>,
    sig: Vec<u8>,
    sig_algorithm: String,
    deployer: Vec<u8>,
}

#[derive(Debug)]
pub struct TransferSend {
    pub signed_contract: Vec<u8>,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}

#[derive(Debug)]
pub enum TransferSendDtoError {
    WrongContract(Vec<u8>),
}

impl From<TransferSendDtoError> for StatusCode {
    fn from(value: TransferSendDtoError) -> Self {
        match value {
            TransferSendDtoError::WrongContract(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl std::fmt::Display for TransferSendDtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<TransferSendDtoError> for poem::Error {
    fn from(value: TransferSendDtoError) -> Self {
        poem::Error::from_string(value.to_string(), value.into())
    }
}

impl TryFrom<TransferSendDto> for TransferSend {
    type Error = TransferSendDtoError;

    fn try_from(value: TransferSendDto) -> Result<Self, Self::Error> {
        if value.transfer_signed_contract.is_empty() {
            return Err(TransferSendDtoError::WrongContract(
                value.transfer_signed_contract,
            ));
        }

        Ok(TransferSend {
            signed_contract: value.transfer_signed_contract,
            sig: value.sig,
            sig_algorithm: value.sig_algorithm,
            deployer: value.deployer,
        })
    }
}

pub struct WalletApi;

#[OpenApi(prefix_path = "/wallet", tag = Tag::Wallet)]
impl WalletApi {
    #[oai(path = "/state/:address", method = "get")]
    async fn wallet_state_and_history(
        &self,
        wallet_address: String,
        client: Data<&FireflyClient>,
    ) -> poem::Result<Json<WalletStateAndHistory>> {
        let wallet_address = wallet_address
            .try_into()
            .map_err(|e| poem::error::ParsePathError)?;

        let v = client.get_state_and_history(wallet_address).await.map(Json);
        v.into()
    }

    #[oai(path = "/transfer/prepare", method = "post")]
    async fn prepare_transfer(
        &self,
        Json(input): Json<PrepareTransferInputDto>,
    ) -> poem::Result<Json<PreparedContract>> {
        input
            .try_into()
            .map(prepare_contract::handle)
            .and_then(|result| result.map_err(PrepareContractRequestProblem::HandlingError))
            .map(Json)
            .map_err(Into::into)
    }

    #[oai(path = "/transfer/send", method = "post")]
    async fn transfer(
        &self,
        Json(body): Json<TransferSendDto>,
        client: Data<&FireflyClient>,
    ) -> poem::Result<Json<TransferResponse>> {
        let transfer = TransferSend::try_from(body)?;

        client
            .0
            .to_owned()
            .transfer(transfer)
            .await
            .map(TransferResponse::from)
            .map(Json)
            .map_err(Into::into)
    }
}
