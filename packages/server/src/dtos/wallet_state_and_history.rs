use firefly_client::models::WalletStateAndHistory;
use poem_openapi::Object;

use super::{boost::BoostDto, exchange::ExchangeDto, request::RequestDto, transfer::TransferDto};

#[derive(Debug, Object)]
pub(crate) struct WalletStateAndHistoryDto {
    pub address: String,
    pub balance: String,
    pub requests: Vec<RequestDto>,
    pub exchanges: Vec<ExchangeDto>,
    pub boosts: Vec<BoostDto>,
    pub transfers: Vec<TransferDto>,
}

impl From<WalletStateAndHistory> for WalletStateAndHistoryDto {
    fn from(value: WalletStateAndHistory) -> Self {
        Self {
            address: value.address.to_string(),
            balance: value.balance.to_string(),
            requests: value.requests.into_iter().map(Into::into).collect(),
            exchanges: value.exchanges.into_iter().map(Into::into).collect(),
            boosts: value.boosts.into_iter().map(Into::into).collect(),
            transfers: value.transfers.into_iter().map(Into::into).collect(),
        }
    }
}
