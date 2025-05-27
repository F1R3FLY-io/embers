use sailfish::TemplateSimple;

use super::Description;
use crate::wallet::models::WalletAddress;

#[derive(TemplateSimple)]
#[template(path = "transfer_contract.rho")]
struct CreateTransferContract {
    wallet_address_from: String,
    wallet_address_to: String,
    amount: u64,
    description: String,
}

pub fn create_transfer_contract(
    from: &WalletAddress,
    to: &WalletAddress,
    amount: u64,
    description: &Description,
) -> String {
    CreateTransferContract {
        wallet_address_from: from.to_string(),
        wallet_address_to: to.to_string(),
        amount,
        description: description.to_string(),
    }
    .render_once()
    .unwrap()
}
