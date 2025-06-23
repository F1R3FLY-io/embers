use anyhow::Context;
use askama::Template;
use firefly_client::WriteNodeClient;
use secp256k1::SecretKey;

use crate::ai_agents::models::InitAgentsEnv;
use crate::wallets::models::InitWalletsEnv;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_contracts(
    client: &mut WriteNodeClient,
    key: &SecretKey,
) -> anyhow::Result<String> {
    let code = InitAgentsEnv.render()?;
    client
        .deploy(key, code)
        .await
        .context("failed to deploy agents env")?;

    let code = InitWalletsEnv.render()?;
    client
        .deploy(key, code)
        .await
        .context("failed to deploy wallets env")?;

    client.propose().await
}
