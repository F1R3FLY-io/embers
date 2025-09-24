use chrono::{DateTime, Utc};
use poem::error::ResponseError;
use poem::http::StatusCode;
use poem_openapi::{Enum, Object};
use structural_convert::StructuralConvert;
use thiserror::Error;

use crate::common::api::dtos::{PreparedContract, Stringified};
use crate::common::models::{PositiveNonZero, WalletAddress};
use crate::wallets::models::{self, DescriptionError};

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
    pub amount: Stringified<PositiveNonZero<i64>>,
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

#[derive(Debug, Clone, Object)]
pub struct Request {
    pub id: String,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<PositiveNonZero<i64>>,
    pub status: RequestStatus,
}

impl From<models::Request> for Request {
    fn from(value: models::Request) -> Self {
        match value {
            models::Request {
                id,
                date,
                amount,
                status,
                ..
            } => Request {
                id: id.into(),
                date: date.into(),
                amount: amount.into(),
                status: status.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Transfer))]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<PositiveNonZero<i64>>,
    pub to_address: Stringified<WalletAddress>,
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
    pub from: Stringified<WalletAddress>,
    pub to: Stringified<WalletAddress>,
    pub amount: Stringified<PositiveNonZero<i64>>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Object)]
pub struct TransferResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Error)]
pub enum TransferValidationError {
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
        let description = value.description.map(TryFrom::try_from).transpose()?;

        Ok(Self {
            from: value.from.0,
            to: value.to.0,
            amount: value.amount.0,
            description,
        })
    }
}
