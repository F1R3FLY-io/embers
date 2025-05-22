use derive_more::Display;

use etc::{WalletAddress, escape_string};
use firefly_client::models::casper::DeployDataProto;

use crate::create_transfer_contract;

#[derive(Debug, Display, Default)]
pub struct Description(String);

impl From<String> for Description {
    fn from(value: String) -> Self {
        Self(escape_string(&value).to_string())
    }
}

#[derive(Debug)]
pub struct PrepareTransferInput {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: u64,
    pub description: Option<Description>,
}

#[derive(Debug)]
pub struct PreparedContract {
    pub contract: Vec<u8>,
}

pub fn prepare_contract(value: PrepareTransferInput) -> PreparedContract {
    use prost::Message as _;

    let code = create_transfer_contract(
        value.from,
        value.to,
        value.amount,
        value.description.unwrap_or_default(),
    );

    let timestamp = chrono::Utc::now().timestamp_millis();
    let contract = DeployDataProto {
        term: code,
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
