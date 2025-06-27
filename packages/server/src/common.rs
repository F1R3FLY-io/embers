use firefly_client::WriteNodeClient;
use firefly_client::models::casper::DeployDataProto;
use firefly_client::models::{BlockId, SignedCode};
use prost::Message;

use crate::common::models::PreparedContract;

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

pub fn prepare_for_signing(code: String) -> PreparedContract {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let contract = DeployDataProto {
        term: code,
        timestamp,
        phlo_price: 1,
        phlo_limit: 1_000_000,
        valid_after_block_number: 0,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract { contract }
}
