use firefly_client::template;

use crate::wallets::models::WalletAddress;

template! {
    #[template(path = "wallet/get_transactions_history.rho")]
    #[derive(Debug, Clone)]
    struct GetUserHistory {
        wallet_address: WalletAddress,
    }
}

pub fn get_user_history_contract(address: WalletAddress) -> anyhow::Result<String> {
    GetUserHistory {
        wallet_address: address,
    }
    .render()
    .map_err(Into::into)
}
