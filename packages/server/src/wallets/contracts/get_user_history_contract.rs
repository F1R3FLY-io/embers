use sailfish::TemplateSimple;

use crate::wallet::models::WalletAddress;

#[derive(TemplateSimple)]
#[template(path = "get_user_history.rho")]
struct GetUserHistory<'a> {
    address: &'a str,
}

pub fn get_user_history_contract(address: &WalletAddress) -> String {
    GetUserHistory {
        address: address.into(),
    }
    .render_once()
    .unwrap()
}
