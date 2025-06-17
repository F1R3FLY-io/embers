use askama::Template;

use crate::{common::rendering::RhoValue, wallets::models::WalletAddress};

#[derive(Template)]
#[template(path = "wallet/get_transactions_history.rho", escape = "none")]
struct GetUserHistory {
    wallet_address: RhoValue<String>,
}

pub fn get_user_history_contract(address: &WalletAddress) -> String {
    GetUserHistory {
        wallet_address: String::from(address.to_owned()).into(),
    }
    .render()
    .unwrap()
}
