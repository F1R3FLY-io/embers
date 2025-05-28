use poem_openapi::Object;

use super::boost::BoostDto;
use super::exchange::ExchangeDto;
use super::request::RequestDto;
use super::transfer::TransferDto;
use crate::common::dtos::Stringified;
use crate::wallet::models::WalletStateAndHistory;

#[derive(Debug, Clone, Object)]
pub struct WalletStateAndHistoryDto {
    pub balance: Stringified<u64>,
    pub requests: Vec<RequestDto>,
    pub exchanges: Vec<ExchangeDto>,
    pub boosts: Vec<BoostDto>,
    pub transfers: Vec<TransferDto>,
}

impl From<WalletStateAndHistory> for WalletStateAndHistoryDto {
    fn from(value: WalletStateAndHistory) -> Self {
        Self {
            balance: value.balance.into(),
            requests: value.requests.into_iter().map(Into::into).collect(),
            exchanges: value.exchanges.into_iter().map(Into::into).collect(),
            boosts: value.boosts.into_iter().map(Into::into).collect(),
            transfers: value.transfers.into_iter().map(Into::into).collect(),
        }
    }
}
