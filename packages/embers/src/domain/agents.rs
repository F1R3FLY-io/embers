use firefly_client::bootstrap::{BootstrapConfig, deploy_and_await};
use firefly_client::helpers::insert_signed_signature;
use firefly_client::models::Uri;
use firefly_client::rendering::Render;
use firefly_client::{NodeEvents, ReadNodeClient, WriteNodeClient};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

mod create;
mod delete;
mod deploy;
mod get;
mod list;
mod list_versions;
pub mod models;
mod save;

#[derive(Clone)]
pub struct AgentsService {
    pub uri: Uri,
    pub write_client: WriteNodeClient,
    pub read_client: ReadNodeClient,
}

#[allow(unused)]
#[derive(Debug, Clone, Render)]
#[template(path = "agents/init.rho", blocks = ["name"])]
struct InitAgentsEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[allow(unused)]
impl AgentsService {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: WriteNodeClient,
        read_client: ReadNodeClient,
        observer_node_events: &NodeEvents,
        deployer_key: &SecretKey,
        env_key: &SecretKey,
        bootstrap_config: &BootstrapConfig,
    ) -> anyhow::Result<Self> {
        let secp = Secp256k1::new();
        let env_public_key = PublicKey::from_secret_key(&secp, env_key);
        let deployer_public_key = PublicKey::from_secret_key(&secp, deployer_key);

        let timestamp = chrono::Utc::now();
        let version = 0;
        let sig = insert_signed_signature(env_key, timestamp, &deployer_public_key, version);
        let env_uri: Uri = env_public_key.into();

        let code = InitAgentsEnv {
            env_uri: env_uri.clone(),
            version,
            public_key: env_public_key.serialize_uncompressed().into(),
            sig,
        }
        .render()?;

        tracing::debug!("code = {code}");

        deploy_and_await(
            &mut write_client,
            deployer_key,
            code,
            timestamp,
            observer_node_events,
            &read_client,
            env_uri.as_ref(),
            "agents",
            bootstrap_config,
        )
        .await?;

        Ok(Self {
            uri: env_uri,
            write_client,
            read_client,
        })
    }
}
