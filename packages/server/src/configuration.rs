use anyhow::Context;
use figment::Figment;
use figment::providers::Env;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub log_level: String,
    pub deploy_service_url: String,
    pub propose_service_url: String,
    pub read_node_url: String,
    pub port: u32,
}

pub fn collect_config() -> anyhow::Result<Config> {
    Figment::new()
        .merge(Env::prefixed("EMBERS__").split("__"))
        .extract()
        .context("failed to collect config")
}
