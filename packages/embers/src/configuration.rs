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
    #[serde(default = "default_recovery_scan_timeout_millis")]
    pub recovery_scan_timeout_millis: u64,
}

const fn default_recovery_poll_millis() -> u64 {
    1_000
}

const fn default_recovery_scan_timeout_millis() -> u64 {
    300_000
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
        config.continuation_store.recovery_scan_timeout_millis > 0,
        "continuation recovery scan timeout must be positive"
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
    // `figment::Jail::expect_with` fixes the closure's return type to
    // `Result<(), figment::Error>`, and figment's error is large; the Err is
    // never constructed here (every closure returns Ok), so the size is moot.
    #![allow(clippy::result_large_err)]

    use figment::Jail;

    use super::*;

    #[test]
    fn finalization_default_is_load_tolerant() {
        let policy = Finalization::default();

        assert_eq!(policy.deploy_timeout(), Duration::from_mins(2));
    }

    /// Populate a jail with a complete, internally-consistent environment so each
    /// test below can flip exactly one variable and observe its effect. Each
    /// `*_KEY` is a distinct valid secp256k1 scalar; the two continuation-store
    /// keys and the AES key are 32-byte hex.
    fn set_full_valid_env(jail: &mut Jail) {
        jail.set_env("EMBERS__ADDRESS", "::1");
        jail.set_env("EMBERS__PORT", "8080");
        jail.set_env("EMBERS__LOG_LEVEL", "info");
        jail.set_env("EMBERS__AES_ENCRYPTION_KEY", "aa".repeat(32));
        jail.set_env("EMBERS__MAINNET__DEPLOY_SERVICE_URL", "http://localhost:40401");
        jail.set_env("EMBERS__MAINNET__VALIDATOR_WS_API_URL", "ws://localhost:40403");
        jail.set_env("EMBERS__MAINNET__OBSERVER_URL", "http://localhost:40413");
        jail.set_env("EMBERS__MAINNET__OBSERVER_WS_API_URL", "ws://localhost:40413");
        jail.set_env("EMBERS__MAINNET__SERVICE_KEY", "01".repeat(32));
        jail.set_env("EMBERS__MAINNET__WALLETS_ENV_KEY", "02".repeat(32));
        jail.set_env("EMBERS__MAINNET__AGENTS_ENV_KEY", "03".repeat(32));
        jail.set_env("EMBERS__MAINNET__AGENTS_TEAMS_ENV_KEY", "04".repeat(32));
        jail.set_env("EMBERS__MAINNET__OSLFS_ENV_KEY", "05".repeat(32));
        jail.set_env("EMBERS__TESTNET__DEPLOY_SERVICE_URL", "http://localhost:50401");
        jail.set_env("EMBERS__TESTNET__VALIDATOR_WS_API_URL", "ws://localhost:50403");
        jail.set_env("EMBERS__TESTNET__OBSERVER_URL", "http://localhost:50413");
        jail.set_env("EMBERS__TESTNET__OBSERVER_WS_API_URL", "ws://localhost:50413");
        jail.set_env("EMBERS__TESTNET__SERVICE_KEY", "06".repeat(32));
        jail.set_env("EMBERS__TESTNET__ENV_KEY", "07".repeat(32));
        jail.set_env("EMBERS__CONTINUATION_STORE__MOUNT_PATH", "/tmp/embers-continuation");
        jail.set_env("EMBERS__CONTINUATION_STORE__ENCRYPTION_KEY", "11".repeat(32));
        jail.set_env("EMBERS__CONTINUATION_STORE__CAPABILITY_KEY", "22".repeat(32));
    }

    #[test]
    fn full_valid_environment_loads() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            let config = collect_config().expect("a complete valid environment must load");
            // Defaults fill in where the environment is silent.
            assert_eq!(config.continuation_store.recovery_poll_millis, 1_000);
            assert_eq!(
                config.continuation_store.recovery_scan_timeout_millis,
                300_000
            );
            assert_eq!(config.finalization.deploy_timeout_millis, 120_000);
            assert_eq!(config.continuation_store.max_object_bytes, None);
            Ok(())
        });
    }

    #[test]
    fn equal_continuation_keys_are_rejected() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            jail.set_env("EMBERS__CONTINUATION_STORE__CAPABILITY_KEY", "11".repeat(32));
            let error = collect_config().expect_err("equal encryption and capability keys must fail");
            assert!(error.to_string().contains("independent"), "{error}");
            Ok(())
        });
    }

    #[test]
    fn zero_recovery_poll_is_rejected() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            jail.set_env("EMBERS__CONTINUATION_STORE__RECOVERY_POLL_MILLIS", "0");
            let error = collect_config().expect_err("a zero recovery poll interval must fail");
            assert!(error.to_string().contains("positive"), "{error}");
            Ok(())
        });
    }

    #[test]
    fn recovery_scan_timeout_is_configurable_and_must_be_positive() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            jail.set_env(
                "EMBERS__CONTINUATION_STORE__RECOVERY_SCAN_TIMEOUT_MILLIS",
                "180000",
            );
            assert_eq!(
                collect_config()
                    .expect("valid recovery timeout")
                    .continuation_store
                    .recovery_scan_timeout_millis,
                180_000
            );
            jail.set_env(
                "EMBERS__CONTINUATION_STORE__RECOVERY_SCAN_TIMEOUT_MILLIS",
                "0",
            );
            let error = collect_config().expect_err("an unbounded scan must be rejected");
            assert!(error.to_string().contains("scan timeout must be positive"));
            Ok(())
        });
    }

    #[test]
    fn zero_deploy_timeout_is_rejected() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            jail.set_env("EMBERS__FINALIZATION__DEPLOY_TIMEOUT_MILLIS", "0");
            let error = collect_config().expect_err("a zero deploy timeout must fail");
            assert!(error.to_string().contains("positive"), "{error}");
            Ok(())
        });
    }

    #[test]
    fn malformed_hex_key_is_rejected() {
        Jail::expect_with(|jail| {
            set_full_valid_env(jail);
            jail.set_env("EMBERS__AES_ENCRYPTION_KEY", "not-valid-hex");
            assert!(
                collect_config().is_err(),
                "a non-hex AES key must fail to deserialize",
            );
            Ok(())
        });
    }
}
