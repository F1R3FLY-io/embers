use std::sync::Arc;

use aes_gcm::{Aes256Gcm, Key};
use anyhow::{Context, anyhow};
use dashmap::DashMap;
use firefly_client::errors::ReadNodeError;
use firefly_client::helpers::insert_signed_signature;
use firefly_client::models::{DeployData, DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{
    NodeEventSource,
    NodeEvents,
    ReadNode,
    ReadNodeClient,
    WriteNode,
    WriteNodeClient,
};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::blockchain;
use crate::domain::agents_teams::models::FireskyCredentials;
use crate::domain::common::deserialize_decrypted;

mod compilation;
mod create;
mod delete;
mod deploy;
mod get;
mod list;
mod list_versions;
pub mod models;
mod publish_to_firesky;
mod run_agents_team;
mod run_on_firesky;
mod save;

#[derive(Clone)]
pub struct AgentsTeamsService<
    R: ReadNode = ReadNodeClient,
    W: WriteNode = WriteNodeClient,
    N: NodeEventSource = NodeEvents,
> {
    pub uri: Uri,
    pub write_client: W,
    pub read_client: R,
    pub observer_node_events: N,
    pub aes_encryption_key: Key<Aes256Gcm>,
    pub firesky_accounts: Arc<DashMap<Uri, FireskyCredentials>>,
}

#[allow(unused)]
#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/init.rho", blocks = ["name"])]
struct InitAgentsTeamsEnv {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[allow(unused)]
#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_firesky_tokens.rho")]
struct GetFireskyTokens {
    env_uri: Uri,
}

#[allow(unused)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    pub async fn bootstrap(
        mut write_client: W,
        read_client: R,
        observer_node_events: N,
        deployer_key: &SecretKey,
        env_key: &SecretKey,
        aes_encryption_key: Key<Aes256Gcm>,
    ) -> anyhow::Result<(Self, DeployId)> {
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

        tracing::info!("agents_teams init deploy code:\n{code}");

        let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

        let deploy_id = write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context("failed to deploy agents teams env")?;

        let code = GetFireskyTokens {
            env_uri: env_uri.clone(),
        }
        .render()?;

        let encrypted_accounts: Result<Vec<blockchain::agents_teams::models::EncryptedMsg>, _> =
            read_client.get_data(code).await;

        let firesky_accounts = match encrypted_accounts {
            Ok(encrypted_accounts) => encrypted_accounts
                .into_iter()
                .map(|account| {
                    deserialize_decrypted::<blockchain::agents_teams::models::FireskyCredentials>(
                        account.into(),
                        &aes_encryption_key,
                    )
                })
                .inspect(|decrypted| {
                    if let Err(err) = decrypted {
                        tracing::info!("failed to decrypt entry: {err}");
                    }
                })
                .flatten()
                .map(|cred| {
                    let uri = cred.uri.try_into()?;
                    Ok((
                        uri,
                        FireskyCredentials {
                            pds_url: cred.pds_url,
                            email: cred.email,
                            token: cred.token,
                        },
                    ))
                })
                .collect::<anyhow::Result<_>>()?,
            Err(ReadNodeError::ReturnValueMissing) => Default::default(),
            Err(err) => return Err(anyhow!(err)),
        };

        Ok((
            Self {
                uri: env_uri,
                write_client,
                read_client,
                observer_node_events,
                aes_encryption_key,
                firesky_accounts: Arc::new(firesky_accounts),
            },
            deploy_id,
        ))
    }
}

#[cfg(test)]
mod template_tests {
    use firefly_client::rendering::Render;

    use super::*;

    #[test]
    fn test_init_template_renders_valid_rholang() {
        let secp = secp256k1::Secp256k1::new();
        let sk = secp256k1::SecretKey::from_byte_array([2u8; 32]).expect("valid key");
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
        let env_uri: Uri = pk.into();

        let code = InitAgentsTeamsEnv {
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

    #[test]
    fn test_get_firesky_tokens_template_renders() {
        let secp = secp256k1::Secp256k1::new();
        let sk = secp256k1::SecretKey::from_byte_array([3u8; 32]).expect("valid key");
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
        let env_uri: Uri = pk.into();

        let code = GetFireskyTokens {
            env_uri: env_uri.clone(),
        }
        .render()
        .expect("template should render");

        assert!(!code.is_empty());
        let uri_str: &str = env_uri.as_ref();
        assert!(code.contains(uri_str));
    }
}
