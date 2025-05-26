use poem_openapi::Object;

use crate::wallet::models::Transfer;

use super::direction::DirectionDto;

#[derive(Debug, Clone, Object)]
pub struct TransferDto {
    pub id: String,
    pub direction: DirectionDto,
    pub date: String,
    pub amount: String,
    pub to_address: String,
    pub cost: String,
}

impl From<Transfer> for TransferDto {
    fn from(value: Transfer) -> Self {
        Self {
            id: value.id,
            direction: value.direction.into(),
            date: value.date.to_string(),
            amount: value.amount.to_string(),
            to_address: value.to_address.to_string(),
            cost: value.cost,
        }
    }
}
