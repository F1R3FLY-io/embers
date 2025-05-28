use chrono::{DateTime, Utc};
use poem_openapi::Object;

use super::direction::DirectionDto;
use crate::common::dtos::Stringified;
use crate::wallet::models::Boost;

#[derive(Debug, Clone, Object)]
pub struct BoostDto {
    pub id: String,
    pub username: String,
    pub direction: DirectionDto,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub post: String,
}

impl From<Boost> for BoostDto {
    fn from(value: Boost) -> Self {
        Self {
            id: value.id,
            username: value.username,
            direction: value.direction.into(),
            date: value.date.into(),
            amount: value.amount.into(),
            post: value.post,
        }
    }
}
