use chrono::{DateTime, Utc};
use poem_openapi::Object;

use super::request_status::RequestStatusDto;
use crate::wallet::models::Request;

#[derive(Debug, Clone, Object)]
pub struct RequestDto {
    pub id: String,
    pub date: DateTime<Utc>,
    pub amount: String,
    pub status: RequestStatusDto,
}

impl From<Request> for RequestDto {
    fn from(value: Request) -> Self {
        Self {
            id: value.id,
            date: value.date,
            amount: value.amount.to_string(),
            status: value.status.into(),
        }
    }
}
