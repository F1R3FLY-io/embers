use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

#[tracing::instrument(level = "info", skip_all, err(Debug))]
#[tracing::instrument(level = "trace", skip(client), ret(Debug))]
pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    client.deploy_signed_contract(contract).await?;
    client.propose().await.map(|_| ())
}
