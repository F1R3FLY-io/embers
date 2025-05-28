use sailfish::TemplateSimple;

use super::Description;
use crate::wallet::models::{Amount, WalletAddress};

#[derive(TemplateSimple)]
#[template(path = "transfer_contract.rho")]
struct CreateTransferContract<'a> {
    wallet_address_from: &'a str,
    wallet_address_to: &'a str,
    amount: u64,
    description: &'a str,
}

pub fn create_transfer_contract(
    from: &WalletAddress,
    to: &WalletAddress,
    amount: Amount,
    description: &Description,
) -> String {
    CreateTransferContract {
        wallet_address_from: from.as_ref(),
        wallet_address_to: to.as_ref(),
        amount: amount.get(),
        description: description.as_ref(),
    }
    .render_once()
    .unwrap()
}
