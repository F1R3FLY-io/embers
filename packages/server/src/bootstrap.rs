use anyhow::Context;
use firefly_client::helpers::insert_signed_signature;
use firefly_client::models::{BlockId, DeployData};
use firefly_client::rendering::{Render, Uri};
use firefly_client::{ReadNodeClient, WriteNodeClient};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::ai_agents::models::InitAgentsEnv;
use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::testnet::handlers::TestnetService;
use crate::wallets::handlers::WalletsService;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_mainnet_contracts(
    client: &mut WriteNodeClient,
    deployer_key: &SecretKey,
) -> anyhow::Result<BlockId> {
    let code = InitAgentsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(deployer_key, deploy_data)
        .await
        .context("failed to deploy agents env")?;

    client.propose().await
}

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents_teams/init.rho")]
struct InitAgentsTeamsEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

impl AgentsTeamsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: WriteNodeClient,
        read_client: ReadNodeClient,
        deployer_key: &SecretKey,
        env_key: &SecretKey,
    ) -> anyhow::Result<Self> {
        let secp = Secp256k1::new();
        let env_public_key = PublicKey::from_secret_key(&secp, env_key);
        let deployer_public_key = PublicKey::from_secret_key(&secp, deployer_key);

        let timestamp = chrono::Utc::now();
        let version = 0;
        let sig = insert_signed_signature(env_key, timestamp, &deployer_public_key, version);
        let env_uri: Uri = env_public_key.into();

        let code = InitAgentsTeamsEnv {
            env_uri: env_uri.clone(),
            version,
            public_key: env_public_key.serialize_uncompressed().into(),
            sig,
        }
        .render()?;
        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context("failed to deploy agents teams env")?;

        Ok(Self {
            uri: env_uri,
            write_client,
            read_client,
        })
    }
}

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/init.rho")]
struct InitWalletsEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

impl WalletsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: WriteNodeClient,
        read_client: ReadNodeClient,
        deployer_key: &SecretKey,
        env_key: &SecretKey,
    ) -> anyhow::Result<Self> {
        let secp = Secp256k1::new();
        let env_public_key = PublicKey::from_secret_key(&secp, env_key);
        let deployer_public_key = PublicKey::from_secret_key(&secp, deployer_key);

        let timestamp = chrono::Utc::now();
        let version = 0;
        let sig = insert_signed_signature(env_key, timestamp, &deployer_public_key, version);
        let env_uri: Uri = env_public_key.into();

        let code = InitWalletsEnv {
            env_uri: env_uri.clone(),
            version,
            public_key: env_public_key.serialize_uncompressed().into(),
            sig,
        }
        .render()?;
        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context("failed to deploy wallets env")?;

        Ok(Self {
            uri: env_uri,
            write_client,
            read_client,
        })
    }
}

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/init.rho")]
struct InitTestnetEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

impl TestnetService {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: WriteNodeClient,
        read_client: ReadNodeClient,
        deployer_key: SecretKey,
        env_key: &SecretKey,
    ) -> anyhow::Result<Self> {
        let secp = Secp256k1::new();
        let env_public_key = PublicKey::from_secret_key(&secp, env_key);
        let deployer_public_key = PublicKey::from_secret_key(&secp, &deployer_key);

        let timestamp = chrono::Utc::now();
        let version = 0;
        let sig = insert_signed_signature(env_key, timestamp, &deployer_public_key, version);
        let env_uri: Uri = env_public_key.into();

        let code = InitTestnetEnv {
            env_uri: env_public_key.into(),
            version,
            public_key: env_public_key.serialize_uncompressed().into(),
            sig,
        }
        .render()?;
        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        write_client
            .deploy(&deployer_key, deploy_data)
            .await
            .context("failed to deploy testnet env")?;

        Ok(Self {
            uri: env_uri,
            service_key: deployer_key,
            write_client,
            read_client,
        })
    }
}
