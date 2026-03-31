use anyhow::Context;
use firefly_client::helpers::insert_signed_signature;
use firefly_client::models::{DeployData, DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEvents, NodeEventSource, ReadNode, ReadNodeClient, WriteNode, WriteNodeClient};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

mod boost;
mod get_wallet_state_and_history;
pub mod models;
mod subscribe_to_deploys;
mod transfer;

#[derive(Clone)]
pub struct WalletsService<
    R: ReadNode = ReadNodeClient,
    W: WriteNode = WriteNodeClient,
    N: NodeEventSource = NodeEvents,
> {
    pub uri: Uri,
    pub write_client: W,
    pub read_client: R,
    pub validator_node_events: N,
    pub observer_node_events: N,
}

#[allow(unused)]
#[derive(Debug, Clone, Render)]
#[template(path = "wallets/init.rho", blocks = ["name"])]
struct InitWalletsEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[allow(unused)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> WalletsService<R, W, N> {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: W,
        read_client: R,
        validator_node_events: N,
        observer_node_events: N,
        deployer_key: &SecretKey,
        env_key: &SecretKey,
    ) -> anyhow::Result<(Self, DeployId)> {
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

        tracing::debug!("code = {code}");

        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        let deploy_id = write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context("failed to deploy wallets env")?;

        Ok((
            Self {
                uri: env_uri,
                write_client,
                read_client,
                validator_node_events,
                observer_node_events,
            },
            deploy_id,
        ))
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    use firefly_client::rendering::Render;

    #[test]
    fn test_init_template_renders_valid_rholang() {
        let secp = secp256k1::Secp256k1::new();
        let sk = secp256k1::SecretKey::from_byte_array([4u8; 32]).expect("valid key");
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
        let env_uri: Uri = pk.into();

        let code = InitWalletsEnv {
            env_uri: env_uri.clone(),
            version: 0,
            public_key: pk.serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render()
        .expect("template should render");

        assert!(!code.is_empty());
        assert!(code.contains("rho:registry:insertSigned:secp256k1"));
        let uri_str: &str = env_uri.as_ref();
        assert!(code.contains(uri_str));
    }
}
