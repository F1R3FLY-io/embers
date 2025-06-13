use askama::Template;

use crate::wallets::models::WalletAddress;

#[derive(Template)]
#[template(path = "wallet/check_balance.rho", escape = "none")]
struct CheckBalance<'a> {
    wallet_address: &'a str,
}

pub fn create_check_balance_contract(address: &WalletAddress) -> String {
    CheckBalance {
        wallet_address: address.as_ref(),
    }
    .render()
    .unwrap()
}
