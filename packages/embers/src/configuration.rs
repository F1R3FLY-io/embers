use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use figment::Figment;
use figment::providers::Env;
use secp256k1::SecretKey;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MainNet {
    pub deploy_service_url: String,
    pub validator_ws_api_url: String,
    pub observer_url: String,
    pub observer_ws_api_url: String,
    pub service_key: SecretKey,
    pub wallets_env_key: SecretKey,
    pub agents_env_key: SecretKey,
    pub agents_teams_env_key: SecretKey,
    pub oslfs_env_key: SecretKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestNet {
    pub deploy_service_url: String,
    pub validator_ws_api_url: String,
    pub observer_url: String,
    pub observer_ws_api_url: String,
    pub service_key: SecretKey,
    pub env_key: SecretKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub address: String,
    pub port: u16,
    pub log_level: String,
    pub mainnet: MainNet,
    pub testnet: TestNet,
    #[serde(deserialize_with = "deserialize_hex_key")]
    pub aes_encryption_key: [u8; 32],
    pub continuation_store: ContinuationStore,
    #[serde(default)]
    pub finalization: Finalization,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Finalization {
    #[serde(default = "default_deploy_timeout_millis")]
    pub deploy_timeout_millis: u64,
}

impl Default for Finalization {
    fn default() -> Self {
        Self {
            deploy_timeout_millis: default_deploy_timeout_millis(),
        }
    }
}

impl Finalization {
    pub const fn deploy_timeout(&self) -> Duration {
        Duration::from_millis(self.deploy_timeout_millis)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContinuationStore {
    pub mount_path: PathBuf,
    #[serde(deserialize_with = "deserialize_hex_key")]
    pub encryption_key: [u8; 32],
    #[serde(deserialize_with = "deserialize_hex_key")]
    pub capability_key: [u8; 32],
    #[serde(default)]
    pub max_object_bytes: Option<u64>,
    #[serde(default = "default_recovery_poll_millis")]
    pub recovery_poll_millis: u64,
}

const fn default_recovery_poll_millis() -> u64 {
    1_000
}

const fn default_deploy_timeout_millis() -> u64 {
    120_000
}

pub fn collect_config() -> anyhow::Result<Config> {
    let config: Config = Figment::new()
        .merge(Env::prefixed("EMBERS__").split("__"))
        .extract()
        .context("failed to collect config")?;
    anyhow::ensure!(
        config.continuation_store.encryption_key != config.continuation_store.capability_key,
        "continuation encryption and capability keys must be independent"
    );
    anyhow::ensure!(
        config.continuation_store.recovery_poll_millis > 0,
        "continuation recovery poll interval must be positive"
    );
    anyhow::ensure!(
        config.finalization.deploy_timeout_millis > 0,
        "deploy finalization timeout must be positive"
    );
    Ok(config)
}

fn deserialize_hex_key<'de, D, const S: usize>(deserializer: D) -> Result<[u8; S], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let mut array = [0u8; S];
    hex::decode_to_slice(&s, &mut array).map_err(serde::de::Error::custom)?;
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalization_default_is_load_tolerant() {
        let policy = Finalization::default();

        assert_eq!(policy.deploy_timeout(), Duration::from_mins(2));
    }
}
