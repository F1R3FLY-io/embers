use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    client.deploy_signed_contract(contract).await?;
    client.propose().await.map(|_| ())
}
