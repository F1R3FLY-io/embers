use askama::Template;

use crate::wallets::models::WalletAddress;

#[derive(Template)]
#[template(path = "wallet/get_user_history.rho", escape = "none")]
struct GetUserHistory<'a> {
    address: &'a str,
}

pub fn get_user_history_contract(address: &WalletAddress) -> String {
    GetUserHistory {
        address: address.into(),
    }
    .render()
    .unwrap()
}
