use derive_more::Display;

use firefly_client::models::casper::DeployDataProto;
use thiserror::Error;

use crate::wallet::{handlers::create_transfer_contract, models::WalletAddress};

#[derive(Debug, Display, Default)]
pub struct Description(String);

const MAX_DESCRIPTION_CHARS_COUNT: usize = 512;

#[derive(Debug, Error)]
pub enum DescriptionError {
    #[error("Maximum chars length")]
    TooLong,
}

impl TryFrom<String> for Description {
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_DESCRIPTION_CHARS_COUNT {
            return Result::Err(DescriptionError::TooLong);
        }

        Ok(Self(html_escape::encode_safe(&value).to_string()))
    }

    type Error = DescriptionError;
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

    let term = create_transfer_contract(
        value.from,
        value.to,
        value.amount,
        value.description.unwrap_or_default(),
    );

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
