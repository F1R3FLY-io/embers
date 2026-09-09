use std::sync::Arc;
use std::time::Duration;

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

/// On-chain registry schema version. Increment whenever the private persistent
/// state or public contract semantics change so an existing environment key is
/// upgraded atomically by `insertSigned` on the next bootstrap.
///
/// Version 3 adds the channel-connector layer: grant records with day-capped
/// run counters, channel bindings, `beginDurableRunUnderGrant`, and the
/// grantee branches of `claimOutputReveal` and `claimCommand`. A version bump
/// deploys a fresh environment instance with empty state channels: development
/// stacks reset with it, while production requires a v2-to-v3 state-migration
/// deploy before rollout.
const AGENTS_TEAMS_ENV_VERSION: i64 = 3;

mod channels;
mod compilation;
pub mod continuation_store;
mod create;
mod delete;
mod deploy;
mod get;
mod get_run;
mod grants;
mod list;
mod list_versions;
pub mod models;
mod orchestration;
pub mod publish_to_firesky;
mod publish_to_mail;
mod published_state;
mod recovery;
mod referencing_teams;
mod reveal_output;
mod run_agents_team;
mod run_commands;
mod run_on_firesky;
mod run_under_grant;
pub mod runtime;
mod save;
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "the testing tree is deliberately pub(crate): the annotation documents the crate-wide sharing intent and stays correct if the domain module ever becomes public"
)]
pub(crate) mod testing;
pub mod trace;
mod unpublish;
pub mod workflow;

pub use grants::GrantError;
pub use publish_to_mail::PublishToMailError;
pub use reveal_output::RevealOutputError;
pub use run_under_grant::RunUnderGrantError;

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
    pub service_key: SecretKey,
    pub aes_encryption_key: Key<Aes256Gcm>,
    pub continuation_store: continuation_store::F1r3driveContinuationStore,
    pub deploy_finalization_timeout: Duration,
    pub run_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub firesky_accounts: Arc<DashMap<Uri, FireskyCredentials>>,
}

pub struct AgentsTeamsRuntimeConfig {
    pub aes_encryption_key: Key<Aes256Gcm>,
    pub continuation_store: continuation_store::F1r3driveContinuationStore,
    pub deploy_finalization_timeout: Duration,
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
        runtime_config: AgentsTeamsRuntimeConfig,
    ) -> anyhow::Result<(Self, DeployId)> {
        let AgentsTeamsRuntimeConfig {
            aes_encryption_key,
            continuation_store,
            deploy_finalization_timeout,
        } = runtime_config;
        let secp = Secp256k1::new();
        let env_public_key = PublicKey::from_secret_key(&secp, env_key);
        let deployer_public_key = PublicKey::from_secret_key(&secp, deployer_key);

        let timestamp = chrono::Utc::now();
        let version = AGENTS_TEAMS_ENV_VERSION;
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
                            handle: cred.handle,
                            did: cred.did,
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
                service_key: *deployer_key,
                aes_encryption_key,
                continuation_store,
                deploy_finalization_timeout,
                run_locks: Arc::new(DashMap::new()),
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
            version: AGENTS_TEAMS_ENV_VERSION,
            public_key: pk.serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render()
        .expect("template should render");

        assert!(!code.is_empty());
        assert!(code.contains("rho:registry:insertSigned:secp256k1"));
        assert!(code.contains("runsCh!({})"));
        assert!(code.contains("for(@runs <- runsCh)"));
        assert!(code.contains("for(@runs <<- runsCh)"));
        assert!(!code.contains("runsMapCh"));

        // Version 3: the channel-connector layer.
        assert!(code.contains("grantsCh!({})"));
        assert!(code.contains("bindingsCh!({})"));
        assert!(code.contains("\"createGrant\""));
        assert!(code.contains("\"revokeGrant\""));
        assert!(code.contains("\"getGrant\""));
        assert!(code.contains("\"listGrantsByGrantor\""));
        assert!(code.contains("\"listGrantsByGrantee\""));
        assert!(code.contains("\"listGrantsForTeam\""));
        assert!(code.contains("\"beginDurableRunUnderGrant\""));
        assert!(code.contains("\"saveChannelBinding\""));
        assert!(code.contains("\"setChannelBindingActive\""));
        assert!(code.contains("\"listChannelBindings\""));
        assert!(code.contains("\"getChannelBindingsForTeam\""));
        // The atomic adjudication join: runs and grants taken together.
        assert!(
            code.contains(
                "for(@address <- deployerAddressCh; @runs <- runsCh; @grants <- grantsCh)"
            )
        );
        // The UTC-epoch-day counter divides the millisecond deploy timestamp.
        assert!(code.contains("timestamp / 86400000"));
        // Ledger datetimes are epoch seconds; deployData stamps are milliseconds.
        assert!(code.contains("timestamp / 1000"));
        // The grantee branches of reveal and command authority.
        assert!(code.contains("grant.get(\"authority\").get(\"reveal\")"));
        assert!(
            code.contains("grant.get(\"authority\").get(\"commands\").contains(command_class)")
        );
        // The public enumeration scrubs the grant link alongside the owner.
        assert!(code.contains("run.delete(\"owner\").delete(\"grant_id\")"));

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
