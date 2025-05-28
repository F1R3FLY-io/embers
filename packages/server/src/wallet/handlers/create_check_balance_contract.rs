use sailfish::TemplateSimple;

use crate::wallet::models::WalletAddress;

#[derive(TemplateSimple)]
#[template(path = "check_balance.rho")]
struct CheckBalance<'a> {
    wallet_address: &'a str,
}

pub fn create_check_balance_contract(address: &WalletAddress) -> String {
    CheckBalance {
        wallet_address: address.as_ref(),
    }
    .render_once()
    .unwrap()
}
