use anyhow::Context;
use figment::Figment;
use figment::providers::Env;
use secp256k1::SecretKey;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MainNet {
    pub deploy_service_url: String,
    pub propose_service_url: String,
    pub read_node_url: String,
    pub service_key: SecretKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestNet {
    pub deploy_service_url: String,
    pub propose_service_url: String,
    pub read_node_url: String,
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
}

pub fn collect_config() -> anyhow::Result<Config> {
    Figment::new()
        .merge(Env::prefixed("EMBERS__").split("__"))
        .extract()
        .context("failed to collect config")
}
