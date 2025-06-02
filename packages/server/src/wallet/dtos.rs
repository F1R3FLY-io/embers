mod prepare_transfer;

use chrono::{DateTime, Utc};
use poem_openapi::{Enum, Object};
use structural_convert::StructuralConvert;

pub use self::prepare_transfer::*;
use crate::common::dtos::Stringified;
use crate::wallet::{handlers, models};

#[derive(Debug, Clone, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::Direction))]
pub enum DirectionDto {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Boost))]
pub struct BoostDto {
    pub id: String,
    pub username: String,
    pub direction: DirectionDto,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub post: String,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Exchange))]
pub struct ExchangeDto {}

#[derive(Debug, Clone, Eq, PartialEq, Enum, StructuralConvert)]
#[oai(rename_all = "lowercase")]
#[convert(from(models::RequestStatus))]
pub enum RequestStatusDto {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Request))]
pub struct RequestDto {
    pub id: String,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub status: RequestStatusDto,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Transfer))]
pub struct TransferDto {
    pub id: String,
    pub direction: DirectionDto,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub to_address: String,
    pub cost: Stringified<u64>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::WalletStateAndHistory))]
pub struct WalletStateAndHistoryDto {
    pub balance: Stringified<u64>,
    pub requests: Vec<RequestDto>,
    pub exchanges: Vec<ExchangeDto>,
    pub boosts: Vec<BoostDto>,
    pub transfers: Vec<TransferDto>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(handlers::PreparedContract))]
pub struct PreparedContractDto {
    pub contract: Vec<u8>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(into(firefly_client::models::SignedCode))]
pub struct TransferSendDto {
    pub contract: Vec<u8>,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
