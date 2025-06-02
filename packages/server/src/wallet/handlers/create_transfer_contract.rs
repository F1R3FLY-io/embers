use sailfish::TemplateSimple;
use uuid::Uuid;

use crate::wallet::{
    contracts::Description,
    models::{Amount, WalletAddress},
};

#[derive(TemplateSimple)]
#[template(path = "send_tokens.rho")]
struct CreateTransferContract<'a> {
    id: &'a str,
    from: &'a str,
    to: &'a str,
    amount: u64,
    description: &'a str,
}

pub fn create_transfer_contract(
    id: Uuid,
    from: &WalletAddress,
    to: &WalletAddress,
    amount: Amount,
    description: &Description,
) -> String {
    let id = id.as_simple().to_string();
    let id = id.as_str();
    let from = from.as_ref();
    let to = to.as_ref();
    let amount = amount.get();
    let description = description.as_ref();

    CreateTransferContract {
        id,
        from,
        to,
        amount,
        description,
    }
    .render_once()
    .unwrap()
}
