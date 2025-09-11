use anyhow::Context;
use firefly_client::models::SignedCode;
use firefly_client::{ReadNodeClient, WriteNodeClient, template};

use crate::ai_agents_teams::handlers::get_agents_team;
use crate::ai_agents_teams::models::{DeployAgentsTeamReq, DeployAgentsTeamResp};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};

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
    fields(address, id, version),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn prepare_deploy_agents_team_contract(
    req: DeployAgentsTeamReq,
    client: &mut WriteNodeClient,
    read_client: &ReadNodeClient,
) -> anyhow::Result<DeployAgentsTeamResp> {
    record_trace!(req);

    let (_graph, phlo_limit) = match req {
        DeployAgentsTeamReq::AgentsTeam {
            id,
            version,
            address,
            phlo_limit,
        } => {
            let graph = get_agents_team(address, id, version, read_client)
                .await?
                .context("agents team not found")?
                .graph
                .context("agents team has no graph")?;

            (graph, phlo_limit)
        }
        DeployAgentsTeamReq::Graph { graph, phlo_limit } => (graph, phlo_limit),
    };

    let code = DeployAiAgentsTeamsDemo {
        name: "demo_agents_team".into(),
    }
    .render()?;

    let valid_after = client.get_head_block_index().await?;
    Ok(DeployAgentsTeamResp {
        contract: prepare_for_signing()
            .code(code)
            .valid_after_block_number(valid_after)
            .phlo_limit(phlo_limit)
            .call(),
    })
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_deploy_agents_team(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await?;
    Ok(())
}
