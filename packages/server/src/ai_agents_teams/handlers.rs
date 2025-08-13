use std::str::FromStr;

use firefly_client::{WriteNodeClient, template};
use secp256k1::SecretKey;

template! {
    #[template(path = "ai_agents_teams/demo.rho")]
    #[derive(Debug, Clone)]
    struct DeployAiAgentsTeamsDemo {
        name: String,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_demo(client: &mut WriteNodeClient, name: String) -> anyhow::Result<()> {
    let key =
        SecretKey::from_str("6a786ec387aff99fcce1bd6faa35916bfad3686d5c98e90a89f77670f535607c")
            .unwrap();

    let contract = DeployAiAgentsTeamsDemo { name }.render()?;
    client.deploy(&key, contract).await?;
    client.propose().await?;
    Ok(())
}
