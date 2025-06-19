use askama::Template;
use firefly_client::WriteNodeClient;
use secp256k1::SecretKey;

#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents/init_agents_env.rho", escape = "none")]
struct InitAgentsEnv;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn init_agents_env(client: &mut WriteNodeClient, key: &SecretKey) -> anyhow::Result<()> {
    let contract = InitAgentsEnv.render()?;
    client.deploy(key, contract).await.map(|_| ())
}
