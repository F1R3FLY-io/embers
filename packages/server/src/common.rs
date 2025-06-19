use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;
use firefly_client::models::casper::DeployDataProto;
use prost::Message;

use crate::common::models::PreparedContract;

use crate::wallets::models::Id;

pub mod bootstrap_contracts;
pub mod dtos;
pub mod models;
pub mod rendering;
pub mod tracing;

pub use bootstrap_contracts::*;

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    client.deploy_signed_contract(contract).await?;
    client.propose().await.map(|_| ())
}

pub fn generate_id() -> Id {
    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::ContextV7::new()))
}

pub fn prepare_for_signing(code: String) -> PreparedContract {
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
