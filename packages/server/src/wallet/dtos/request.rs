use chrono::{DateTime, Utc};
use poem_openapi::Object;

use super::request_status::RequestStatusDto;
use crate::common::dtos::Stringified;
use crate::wallet::models::Request;

#[derive(Debug, Clone, Object)]
pub struct RequestDto {
    pub id: String,
    pub date: Stringified<DateTime<Utc>>,
    pub amount: Stringified<u64>,
    pub status: RequestStatusDto,
}

impl From<Request> for RequestDto {
    fn from(value: Request) -> Self {
        Self {
            id: value.id,
            date: value.date.into(),
            amount: value.amount.into(),
            status: value.status.into(),
        }
    }
}
