use anyhow::Context;
use figment::Figment;
use figment::providers::Env;
use secp256k1::SecretKey;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub deploy_service_url: String,
    pub propose_service_url: String,

    pub read_node_url: String,

    pub default_wallet_address: String,
    pub default_wallet_key: SecretKey,
}

pub fn collect_config() -> anyhow::Result<Config> {
    Figment::new()
        .merge(Env::prefixed("EMBERS__").split("__"))
        .extract()
        .context("failed to collect config")
}
