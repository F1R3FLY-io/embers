use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use secp256k1::SecretKey;

use crate::models::{DeployData, DeployId};
use crate::read_node_client::ReadNodeClient;
use crate::write_node_client::WriteNodeClient;

/// Configuration for bootstrap deploy finalization.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Maximum time to wait for deploy finalization (includes block inclusion + finalization).
    pub finalization_timeout: Duration,
    /// Maximum number of deploy+wait attempts before giving up.
    pub max_deploy_attempts: u32,
    /// Maximum time to poll observer for readability after finalization.
    pub observer_poll_timeout: Duration,
    /// Interval between poll attempts.
    pub poll_interval: Duration,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            finalization_timeout: Duration::from_secs(120),
            max_deploy_attempts: 3,
            observer_poll_timeout: Duration::from_secs(30),
            poll_interval: Duration::from_secs(2),
        }
    }
}

/// Deploys an init contract and waits for finalization + observer readability.
///
/// Uses HTTP polling (find_deploy + is_finalized) instead of WebSocket events
/// for reliability. Retries the full deploy cycle up to `config.max_deploy_attempts` times.
///
/// The deploy is idempotent — re-deploying the same init contract against
/// existing blockchain state is a no-op due to the version check in insert_signed.rho.
pub async fn deploy_and_await(
    write_client: &mut WriteNodeClient,
    deployer_key: &SecretKey,
    code: String,
    timestamp: DateTime<Utc>,
    read_client: &ReadNodeClient,
    env_uri: &str,
    service_name: &str,
    config: &BootstrapConfig,
) -> anyhow::Result<DeployId> {
    let mut last_error = None;

    for attempt in 1..=config.max_deploy_attempts {
        tracing::info!(
            service = service_name,
            attempt,
            max_attempts = config.max_deploy_attempts,
            "deploying init contract"
        );

        let deploy_data = DeployData::builder(code.clone())
            .timestamp(timestamp)
            .build();

        let deploy_id = write_client
            .deploy(deployer_key, deploy_data)
            .await
            .context(format!("[{service_name}] failed to submit deploy"))?;

        tracing::info!(
            service = service_name,
            deploy_id = %deploy_id,
            "deploy submitted"
        );

        match await_deploy_finalization(
            &deploy_id,
            read_client,
            env_uri,
            service_name,
            config,
        )
        .await
        {
            Ok(()) => return Ok(deploy_id),
            Err(err) => {
                tracing::warn!(
                    service = service_name,
                    attempt,
                    "deploy finalization failed: {err}"
                );
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!("[{service_name}] deploy failed after all attempts")
    }))
}

/// Waits for a deploy to be included in a finalized block via HTTP polling,
/// then verifies the contract is readable on the observer.
///
/// Three phases:
/// 1. Poll GET /api/deploy/{id} until deploy is found in a block
/// 2. Poll GET /api/is-finalized/{block_hash} until block is finalized
/// 3. Poll explore-deploy until registry lookup succeeds
async fn await_deploy_finalization(
    deploy_id: &DeployId,
    read_client: &ReadNodeClient,
    env_uri: &str,
    service_name: &str,
    config: &BootstrapConfig,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + config.finalization_timeout;

    // Phase 1: Wait for deploy to be included in a block
    tracing::info!(
        service = service_name,
        deploy_id = %deploy_id,
        "waiting for deploy to be included in a block"
    );

    let block_hash = loop {
        match read_client.find_deploy(deploy_id).await {
            Ok(Some(hash)) => {
                tracing::info!(
                    service = service_name,
                    deploy_id = %deploy_id,
                    block_hash = %hash,
                    "deploy found in block"
                );
                break hash;
            }
            Ok(None) => {}
            Err(err) => {
                tracing::debug!(
                    service = service_name,
                    "find_deploy error (retrying): {err}"
                );
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "[{service_name}] deploy {deploy_id} not included in a block after {:?}",
                config.finalization_timeout
            ));
        }
        tokio::time::sleep(config.poll_interval).await;
    };

    // Phase 2: Wait for block finalization
    tracing::info!(
        service = service_name,
        block_hash = %block_hash,
        "waiting for block finalization"
    );

    loop {
        match read_client.is_finalized(&block_hash).await {
            Ok(true) => {
                tracing::info!(
                    service = service_name,
                    block_hash = %block_hash,
                    "block finalized"
                );
                break;
            }
            Ok(false) => {}
            Err(err) => {
                tracing::debug!(
                    service = service_name,
                    "is_finalized error (retrying): {err}"
                );
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "[{service_name}] block {block_hash} not finalized after {:?}",
                config.finalization_timeout
            ));
        }
        tokio::time::sleep(config.poll_interval).await;
    }

    // Phase 3: Verify contract readability on observer
    tracing::info!(
        service = service_name,
        "verifying observer readability"
    );

    let readability_deadline = tokio::time::Instant::now() + config.observer_poll_timeout;
    let verification_code = format!(
        r#"new ret, rl(`rho:registry:lookup`) in {{ rl!(`{env_uri}`, *ret) }}"#
    );

    loop {
        let result: Result<serde_json::Value, _> =
            read_client.get_data(verification_code.clone()).await;

        match result {
            Ok(_) => {
                tracing::info!(
                    service = service_name,
                    "contract verified readable on observer"
                );
                return Ok(());
            }
            Err(err) => {
                if tokio::time::Instant::now() >= readability_deadline {
                    return Err(anyhow::anyhow!(
                        "[{service_name}] contract at {env_uri} not readable on observer \
                         after {:?} of polling (last error: {err})",
                        config.observer_poll_timeout
                    ));
                }
                tracing::debug!(
                    service = service_name,
                    "observer not yet synced, retrying: {err}",
                );
                tokio::time::sleep(config.poll_interval).await;
            }
        }
    }
}
