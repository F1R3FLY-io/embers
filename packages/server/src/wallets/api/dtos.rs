use chrono::{DateTime, Utc};
use poem::error::ResponseError;
use poem::http::StatusCode;
use poem_openapi::{Enum, Object};
use structural_convert::StructuralConvert;
use thiserror::Error;

use crate::common::api::dtos::Stringified;
use crate::common::models::ParseWalletAddressError;
use crate::wallets::models;
use crate::wallets::models::DescriptionError;

#[derive(Debug, Clone, Eq, PartialEq, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::Direction))]
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

#[derive(Debug, Clone, Eq, PartialEq, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::RequestStatus))]
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
pub struct PrepareTransferInput {
    from: String,
    to: String,
    amount: Stringified<i64>,
    description: Option<String>,
}

#[derive(Debug, Clone, Error)]
pub enum TransferValidationError {
    #[error("amount field can't be empty")]
    EmptyAmount,
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

impl TryFrom<PrepareTransferInput> for models::PrepareTransferInput {
    type Error = TransferValidationError;

    fn try_from(value: PrepareTransferInput) -> Result<Self, Self::Error> {
        let to = value
            .to
            .try_into()
            .map_err(Self::Error::WrongReceiverAddressFormat)?;

        let from = value
            .from
            .try_into()
            .map_err(Self::Error::WrongSenderAddressFormat)?;

        let amount = value
            .amount
            .0
            .try_into()
            .map_err(|_| Self::Error::EmptyAmount)?;
        let description = value.description.map(TryFrom::try_from).transpose()?;

        Ok(Self {
            from,
            to,
            amount,
            description,
        })
    }
}
