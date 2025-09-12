use firefly_client::WriteNodeClient;
use firefly_client::models::casper::DeployDataProto;
use firefly_client::models::{BlockId, SignedCode};
use prost::Message;

use crate::common::models::{PositiveNonZero, PreparedContract};

pub mod api;
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
) -> PreparedContract {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let contract = DeployDataProto {
        term: code,
        timestamp,
        phlo_price: 1,
        phlo_limit: phlo_limit.map_or(500_000, |v| v.0),
        valid_after_block_number: valid_after_block_number as _,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract(contract)
}
