use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

use crate::wallets::models::Id;

pub mod bootstrap_contracts;
pub mod dtos;
pub mod models;
pub mod rendering;

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
