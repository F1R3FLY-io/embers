use chrono::{DateTime, Utc};
use poem::error::ResponseError;
use poem::http::StatusCode;
use poem_openapi::{Enum, Object};
use structural_convert::StructuralConvert;
use thiserror::Error;

use crate::common::api::dtos::{PreparedContract, Stringified};
use crate::common::models::ParseWalletAddressError;
use crate::wallets::models::{self, DescriptionError, PositiveNonZeroParsingError};

#[derive(Debug, Clone, Eq, PartialEq, StructuralConvert, Enum)]
#[convert(from(models::Direction))]
#[oai(rename_all = "lowercase")]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Boost))]
pub struct Boost {
    pub id: String,
    pub username: String,
    pub direction: Direction,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub post: String,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Exchange))]
pub struct Exchange {}

#[derive(Debug, Clone, Eq, PartialEq, StructuralConvert, Enum)]
#[convert(from(models::RequestStatus))]
#[oai(rename_all = "lowercase")]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Request))]
pub struct Request {
    pub id: String,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Transfer))]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub to_address: String,
    pub cost: Stringified<u64>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::WalletStateAndHistory))]
pub struct WalletStateAndHistory {
    pub balance: Stringified<u64>,
    pub requests: Vec<Request>,
    pub exchanges: Vec<Exchange>,
    pub boosts: Vec<Boost>,
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone, Object)]
pub struct TransferReq {
    from: String,
    to: String,
    amount: Stringified<i64>,
    description: Option<String>,
}

#[derive(Debug, Clone, Object)]
pub struct TransferResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Error)]
pub enum TransferValidationError {
    #[error("description format error: {0}")]
    AmountError(#[from] PositiveNonZeroParsingError),
    #[error("receiver wallet adress has wrong format: {0}")]
    WrongReceiverAddressFormat(ParseWalletAddressError),
    #[error("sender wallet adress has wrong format: {0}")]
    WrongSenderAddressFormat(ParseWalletAddressError),
    #[error("description format error: {0}")]
    DescriptionError(#[from] DescriptionError),
}

impl ResponseError for TransferValidationError {
    fn status(&self) -> poem::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl TryFrom<TransferReq> for models::PrepareTransferInput {
    type Error = TransferValidationError;

    fn try_from(value: TransferReq) -> Result<Self, Self::Error> {
        let to = value
            .to
            .try_into()
            .map_err(Self::Error::WrongReceiverAddressFormat)?;

        let from = value
            .from
            .try_into()
            .map_err(Self::Error::WrongSenderAddressFormat)?;

        let amount = value.amount.0.try_into()?;
        let description = value.description.map(TryFrom::try_from).transpose()?;

        Ok(Self {
            from,
            to,
            amount,
            description,
        })
    }
}
