use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use secp256k1::SecretKey;

use crate::models::{DeployData, DeployId};
use crate::node_events::NodeEvents;
use crate::read_node_client::ReadNodeClient;
use crate::write_node_client::WriteNodeClient;

/// Configuration for bootstrap deploy finalization.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Maximum time to wait for a single deploy finalization.
    pub finalization_timeout: Duration,
    /// Maximum number of deploy+wait attempts before giving up.
    pub max_deploy_attempts: u32,
    /// Maximum time to poll observer for readability after finalization.
    pub observer_poll_timeout: Duration,
    /// Interval between observer readability poll attempts.
    pub observer_poll_interval: Duration,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            finalization_timeout: Duration::from_secs(120),
            max_deploy_attempts: 3,
            observer_poll_timeout: Duration::from_secs(30),
            observer_poll_interval: Duration::from_secs(2),
        }
    }
}

/// Deploys an init contract and waits for finalization + observer readability.
///
/// Retries the full deploy cycle up to `config.max_deploy_attempts` times.
/// Each attempt: submit deploy → wait for BlockFinalised → poll observer until readable.
///
/// The deploy is idempotent — re-deploying the same init contract against
/// existing blockchain state is a no-op due to the version check in insert_signed.rho.
pub async fn deploy_and_await(
    write_client: &mut WriteNodeClient,
    deployer_key: &SecretKey,
    code: String,
    timestamp: DateTime<Utc>,
    observer_node_events: &NodeEvents,
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
            observer_node_events,
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

/// Waits for a deploy to be finalized, then verifies the contract is readable
/// on the observer node.
async fn await_deploy_finalization(
    deploy_id: &DeployId,
    observer_node_events: &NodeEvents,
    read_client: &ReadNodeClient,
    env_uri: &str,
    service_name: &str,
    config: &BootstrapConfig,
) -> anyhow::Result<()> {
    tracing::info!(
        service = service_name,
        deploy_id = %deploy_id,
        "waiting for deploy finalization"
    );

    let finalized = observer_node_events
        .wait_for_deploy(deploy_id, config.finalization_timeout)
        .await;

    if !finalized {
        return Err(anyhow::anyhow!(
            "[{service_name}] deploy {deploy_id} finalization timed out after {:?}",
            config.finalization_timeout
        ));
    }

    // Check if the deploy execution errored
    if let Some(errored) = observer_node_events.deploy_errored(deploy_id) {
        if errored {
            return Err(anyhow::anyhow!(
                "[{service_name}] deploy {deploy_id} finalized but Rholang execution errored"
            ));
        }
        tracing::info!(
            service = service_name,
            deploy_id = %deploy_id,
            "deploy finalized successfully (errored=false)"
        );
    } else {
        tracing::info!(
            service = service_name,
            deploy_id = %deploy_id,
            "deploy finalized (error status unknown)"
        );
    }

    tracing::info!(
        service = service_name,
        deploy_id = %deploy_id,
        "verifying observer readability"
    );

    let verification_code = format!(
        r#"new ret, rl(`rho:registry:lookup`) in {{ rl!(`{env_uri}`, *ret) }}"#
    );

    let deadline = tokio::time::Instant::now() + config.observer_poll_timeout;
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
                if tokio::time::Instant::now() >= deadline {
                    return Err(anyhow::anyhow!(
                        "[{service_name}] contract at {env_uri} not readable on observer \
                         after {:?} of polling (last error: {err})",
                        config.observer_poll_timeout
                    ));
                }
                tracing::debug!(
                    service = service_name,
                    "observer not yet synced, retrying in {:?}: {err}",
                    config.observer_poll_interval
                );
                tokio::time::sleep(config.observer_poll_interval).await;
            }
        }
    }
}
