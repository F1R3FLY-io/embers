use firefly_client::models::Exchange;
use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct ExchangeDto {}

impl From<Exchange> for ExchangeDto {
    fn from(_value: Exchange) -> Self {
        Self {}
    }
}
