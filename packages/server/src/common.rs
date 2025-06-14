use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

pub mod dtos;
pub mod models;
pub mod rendering;
pub mod tracing;

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    client.deploy_signed_contract(contract).await?;
    client.propose().await.map(|_| ())
}
