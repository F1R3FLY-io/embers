use poem_openapi::Object;

use crate::wallet::models::Exchange;

#[derive(Debug, Clone, Object)]
pub struct ExchangeDto {}

impl From<Exchange> for ExchangeDto {
    fn from(_value: Exchange) -> Self {
        Self {}
    }
}
