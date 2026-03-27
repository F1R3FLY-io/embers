use anyhow::Context;
use figment::Figment;
use figment::providers::Env;
use secp256k1::SecretKey;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MainNet {
    pub deploy_service_url: String,
    pub propose_service_url: String,
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
#[allow(dead_code)]
pub struct TestNet {
    pub deploy_service_url: String,
    pub propose_service_url: String,
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
    /// Max seconds to wait for each init contract finalization during bootstrap (default 120)
    #[serde(default = "default_bootstrap_timeout")]
    pub bootstrap_timeout_secs: u64,
    /// Max attempts to wait for observer explore-deploy to reflect finalized state (default 15)
    #[serde(default = "default_observer_sync_attempts")]
    pub observer_sync_attempts: u32,
    /// Interval in seconds between observer sync attempts (default 2)
    #[serde(default = "default_observer_sync_interval_secs")]
    pub observer_sync_interval_secs: u64,
    /// Max seconds to wait for deploy finalization in send endpoints (default 120)
    #[serde(default = "default_deploy_finalization_timeout_secs")]
    pub deploy_finalization_timeout_secs: u64,
    /// Max attempts to read result after finalization (default 15)
    #[serde(default = "default_read_after_finalize_attempts")]
    pub read_after_finalize_attempts: u32,
}

fn default_bootstrap_timeout() -> u64 { 120 }
fn default_observer_sync_attempts() -> u32 { 15 }
fn default_observer_sync_interval_secs() -> u64 { 2 }
fn default_deploy_finalization_timeout_secs() -> u64 { 120 }
fn default_read_after_finalize_attempts() -> u32 { 15 }

pub fn collect_config() -> anyhow::Result<Config> {
    Figment::new()
        .merge(Env::prefixed("EMBERS__").split("__"))
        .extract()
        .context("failed to collect config")
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
