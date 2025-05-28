use poem_openapi::Enum;

use crate::wallet::models::RequestStatus;

#[derive(Debug, Clone, Eq, PartialEq, Enum)]
#[oai(rename_all = "lowercase")]
pub enum RequestStatusDto {
    Done,
    Ongoing,
    Cancelled,
}

impl From<RequestStatus> for RequestStatusDto {
    fn from(value: RequestStatus) -> Self {
        match value {
            RequestStatus::Done => Self::Done,
            RequestStatus::Ongoing => Self::Ongoing,
            RequestStatus::Cancelled => Self::Cancelled,
        }
    }
}
