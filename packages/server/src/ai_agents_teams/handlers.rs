use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;
use firefly_client::{ReadNodeClient, WriteNodeClient, template};
use secp256k1::SecretKey;

template! {
    #[template(path = "ai_agents_teams/deploy_demo.rho")]
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

    let deploy_data = DeployAiAgentsTeamsDemo { name }.builder()?.build();
    client.deploy(&key, deploy_data).await?;
    client.propose().await?;
    Ok(())
}

template! {
    #[template(path = "ai_agents_teams/run_demo.rho")]
    #[derive(Debug, Clone)]
    struct RunAiAgentsTeamsDemo {
        name: String,
        prompt: String,
    }
}

template! {
    #[template(path = "ai_agents_teams/get_demo_result.rho")]
    #[derive(Debug, Clone)]
    struct GetAiAgentsTeamsDemoResult {
        prompt: String,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn run_demo(
    client: &mut WriteNodeClient,
    read_client: &ReadNodeClient,
    name: String,
    prompt: String,
) -> anyhow::Result<serde_json::Value> {
    let key =
        SecretKey::from_str("6a786ec387aff99fcce1bd6faa35916bfad3686d5c98e90a89f77670f535607c")
            .unwrap();

    let deploy_data = RunAiAgentsTeamsDemo {
        name,
        prompt: prompt.clone(),
    }
    .builder()?
    .phlo_limit(500_000_000)
    .build();

    client.deploy(&key, deploy_data).await?;
    let block_hash = client.propose().await?;

    let finalized = read_client
        .wait_finalization(block_hash, Duration::from_secs(1500))
        .await?;

    if !finalized {
        return Err(anyhow!("block is not finalized"));
    }

    let code = GetAiAgentsTeamsDemoResult { prompt }.render()?;
    read_client.get_data(code).await.map_err(Into::into)
}
