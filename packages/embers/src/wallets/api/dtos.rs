use chrono::{DateTime, Utc};
use firefly_client::models::WalletAddress;
use poem::error::ResponseError;
use poem::http::StatusCode;
use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;
use thiserror::Error;

use crate::common::api::dtos::{PreparedContract, Stringified};
use crate::common::models::PositiveNonZero;
use crate::wallets::models;

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

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Request))]
pub struct Request {
    pub id: String,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<PositiveNonZero<i64>>,
    pub status: RequestStatus,
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
    DescriptionError(#[from] models::DescriptionError),
}

impl ResponseError for TransferValidationError {
    fn status(&self) -> poem::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl TryFrom<TransferReq> for models::TransferReq {
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

#[derive(Debug, Clone, Enum, StructuralConvert)]
#[convert(from(models::NodeType))]
pub enum NodeType {
    Validator,
    Observer,
}

#[derive(Debug, Clone, Object)]
pub struct DeploySeen {
    pub deploy_id: String,
    pub cost: u64,
    pub errored: bool,
    pub node_type: NodeType,
}

#[derive(Debug, Clone, Union)]
#[oai(discriminator_name = "type")]
pub enum WalletEvent {
    DeploySeen(DeploySeen),
}

impl From<models::WalletEvent> for WalletEvent {
    fn from(value: models::WalletEvent) -> Self {
        match value {
            models::WalletEvent::DeploySeen {
                deploy_id,
                cost,
                errored,
                node_type,
            } => Self::DeploySeen(DeploySeen {
                deploy_id: deploy_id.into(),
                cost,
                errored,
                node_type: node_type.into(),
            }),
        }
    }
}
