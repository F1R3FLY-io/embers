use anyhow::Context;
use firefly_client::helpers::insert_signed_signature;
use firefly_client::models::{DeployData, Uri};
use firefly_client::rendering::Render;
use firefly_client::{ReadNodeClient, WriteNodeClient};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

mod create;
mod delete;
mod get;
mod list;
mod list_versions;
pub mod models;
mod save;

#[derive(Clone)]
pub struct OslfsService {
    pub uri: Uri,
    pub write_client: WriteNodeClient,
    pub read_client: ReadNodeClient,
}

#[allow(unused)]
#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/init.rho", blocks = ["name"])]
struct InitEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[allow(unused)]
impl OslfsService {
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

        let code = InitEnv {
            env_uri: env_public_key.into(),
            version,
            public_key: env_public_key.serialize_uncompressed().into(),
            sig,
        }
        .render()?;

        tracing::debug!("code = {code}");

        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context("failed to deploy oslf env")?;

        Ok(Self {
            uri: env_uri,
            write_client,
            read_client,
        })
    }
}
