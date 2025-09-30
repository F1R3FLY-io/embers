use anyhow::Context;
use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::{BlockId, DeployData};
use secp256k1::SecretKey;

use crate::ai_agents::models::InitAgentsEnv;
use crate::ai_agents_teams::models::InitAgentsTeamsEnv;
use crate::testnet::models::InitTestnetEnv;
use crate::wallets::models::InitWalletsEnv;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_mainnet_contracts(
    client: &mut WriteNodeClient,
    key: &SecretKey,
) -> anyhow::Result<BlockId> {
    let code = InitAgentsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy agents env")?;

    let code = InitAgentsTeamsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy agents teams env")?;

    let code = InitWalletsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy wallets env")?;

    client.propose().await
}

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_testnet_contracts(
    client: &mut WriteNodeClient,
    key: &SecretKey,
) -> anyhow::Result<BlockId> {
    let code = InitTestnetEnv.render()?;
    let deploy_data = DeployData::builder(code).build();

    client
        .deploy(key, deploy_data)
        .await
        .context("failed to testnet env")?;

    client.propose().await
}
