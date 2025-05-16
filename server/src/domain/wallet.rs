use super::models::{Cost, WalletStateAndHistory};
use crate::storage::firefly::FireflyApi;

#[derive(Clone, derive_more::Constructor)]
pub struct WalletService {
    default_wallet_address: String,
    firefly: FireflyApi,
}

impl WalletService {
    pub async fn get_state_and_history(&self) -> anyhow::Result<WalletStateAndHistory> {
        self.firefly
            .get_state_and_history(self.default_wallet_address.clone())
            .await
    }

    pub async fn transfer(
        &mut self,
        amount: u64,
        to_address: String,
        description: Option<String>,
    ) -> anyhow::Result<Cost> {
        self.firefly
            .transfer(
                amount,
                &self.default_wallet_address,
                &to_address,
                description.as_deref(),
            )
            .await
    }
}
