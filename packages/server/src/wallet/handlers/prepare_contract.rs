use firefly_client::models::casper::DeployDataProto;
use prost::Message as _;
use sailfish::TemplateSimple;

use crate::wallet::models::{Amount, Description, WalletAddress};

#[derive(TemplateSimple)]
#[template(path = "transfer_contract.rho")]
struct TransferContract<'a> {
    wallet_address_from: &'a str,
    wallet_address_to: &'a str,
    amount: u64,
    description: &'a str,
}

#[derive(Debug, Clone)]
pub struct PrepareTransferInput {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}

#[derive(Debug, Clone)]
pub struct PreparedContract {
    pub contract: Vec<u8>,
}

#[tracing::instrument(level = "info", skip_all)]
#[tracing::instrument(level = "trace", ret(Debug))]
pub fn prepare_transfer_contract(value: PrepareTransferInput) -> PreparedContract {
    let term = TransferContract {
        wallet_address_from: value.from.as_ref(),
        wallet_address_to: value.to.as_ref(),
        amount: value.amount.get(),
        description: value.description.unwrap_or_default().as_ref(),
    }
    .render_once()
    .unwrap();

    let timestamp = chrono::Utc::now().timestamp_millis();
    let contract = DeployDataProto {
        term,
        timestamp,
        phlo_price: 1,
        phlo_limit: 500_000,
        valid_after_block_number: 0,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract { contract }
}
