use chrono::{DateTime, Utc};
use poem_openapi::Object;

use super::direction::DirectionDto;
use crate::common::dtos::Stringified;
use crate::wallet::models::Transfer;

#[derive(Debug, Clone, Object)]
pub struct TransferDto {
    pub id: String,
    pub direction: DirectionDto,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub to_address: String,
    pub cost: Stringified<u64>,
}

impl From<Transfer> for TransferDto {
    fn from(value: Transfer) -> Self {
        Self {
            id: value.id,
            direction: value.direction.into(),
            date: value.date.into(),
            amount: value.amount.into(),
            to_address: value.to_address.into(),
            cost: value.cost.into(),
        }
    }
}
