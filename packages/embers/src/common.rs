use chrono::{DateTime, Utc};
use firefly_client::WriteNodeClient;
use firefly_client::models::casper::DeployDataProto;
use firefly_client::models::{BlockId, SignedCode};
use prost::Message;

use crate::common::models::{PositiveNonZero, PreparedContract};

pub mod api;
pub mod blockchain;
pub mod models;
pub mod tracing;

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<BlockId> {
    client.deploy_signed_contract(contract).await?;
    client.propose().await
}

#[bon::builder]
pub fn prepare_for_signing(
    code: String,
    valid_after_block_number: u64,
    phlo_limit: Option<PositiveNonZero<i64>>,
    timestamp: Option<DateTime<Utc>>,
) -> PreparedContract {
    let timestamp = timestamp.unwrap_or_else(|| chrono::Utc::now());
    let contract = DeployDataProto {
        term: code,
        timestamp: timestamp.timestamp_millis(),
        phlo_price: 1,
        phlo_limit: phlo_limit.map_or(500_000, |v| v.0),
        valid_after_block_number: valid_after_block_number as _,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract(contract)
}
