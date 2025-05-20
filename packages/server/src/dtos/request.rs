use firefly_client::models::Request;
use poem_openapi::Object;

use super::request_status::RequestStatusDto;

#[derive(Debug, Clone, Object)]
pub struct RequestDto {
    pub id: String,
    pub date: String,
    pub amount: String,
    pub status: RequestStatusDto,
}

impl From<Request> for RequestDto {
    fn from(value: Request) -> Self {
        Self {
            id: value.id,
            date: value.date,
            amount: value.amount,
            status: value.status.into(),
        }
    }
}
