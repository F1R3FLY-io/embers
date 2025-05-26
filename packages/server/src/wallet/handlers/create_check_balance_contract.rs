use sailfish::TemplateSimple;

use crate::wallet::models::WalletAddress;

#[derive(TemplateSimple)]
#[template(path = "check_balance.rho")]
struct CheckBalance {
    wallet_address: String,
}

pub fn create_check_balance_contract(address: &WalletAddress) -> String {
    CheckBalance {
        wallet_address: address.to_string(),
    }
    .render_once()
    .unwrap()
}
