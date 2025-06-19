use firefly_client::WriteNodeClient;
use secp256k1::SecretKey;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_contracts(
    client: &mut WriteNodeClient,
    key: &SecretKey,
    contracts: Vec<String>,
) -> anyhow::Result<String> {
    for contract in contracts {
        client.deploy(key, contract).await?;
    }

    client.propose().await
}
