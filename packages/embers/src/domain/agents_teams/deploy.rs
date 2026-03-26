use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::compilation::{parse, render};
use crate::domain::agents_teams::models::{DeployReq, DeployResp, DeploySignedReq};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/record_deploy.rho")]
struct RecordDeploy {
    env_uri: Uri,
    id: String,
    version: String,
    last_deploy: DateTime<Utc>,
    uri: Uri,
}

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_deploy_contract(
        &self,
        request: DeployReq,
        valid_after: Option<u64>,
    ) -> anyhow::Result<DeployResp> {
        record_trace!(request);

        let (graph, phlo_limit, deploy, system) = match request {
            DeployReq::AgentsTeam {
                id,
                version,
                address,
                phlo_limit,
                deploy,
            } => {
                // Retry because explore-deploy on observer may lag behind finalized state
                let mut agents_team = None;
                for attempt in 1..=15u32 {
                    match self.get(address.clone(), id.clone(), version.clone()).await {
                        Ok(Some(team)) => { agents_team = Some(team); break; }
                        _ => if attempt < 15 {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        }
                    }
                }
                let agents_team = agents_team.context("agents team not found after 30s")?;
                let graph = agents_team.graph.context("agents team has no graph")?;

                // Use client-provided block number, fall back to validator head
                let va = match valid_after {
                    Some(n) => n,
                    None => self.write_client.clone().get_head_block_index().await?,
                };

                let system_code = RecordDeploy {
                    env_uri: self.uri.clone(),
                    id,
                    version: agents_team.version,
                    last_deploy: Utc::now(),
                    uri: deploy.uri_pub_key.into(),
                }
                .render()?;

                (
                    graph,
                    phlo_limit,
                    deploy,
                    Some(
                        prepare_for_signing()
                            .code(system_code)
                            .valid_after_block_number(va)
                            .call(),
                    ),
                )
            }
            DeployReq::Graph {
                graph,
                phlo_limit,
                deploy,
            } => (graph, phlo_limit, deploy, None),
        };

        let timestamp = deploy.timestamp;

        let code = parse(&graph)?;
        let code = render(code, deploy)?;

        // Use client-provided block number, fall back to validator head
        let va = match valid_after {
            Some(n) => n,
            None => self.write_client.clone().get_head_block_index().await?,
        };

        Ok(DeployResp {
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(va)
                .timestamp(timestamp)
                .phlo_limit(phlo_limit)
                .call(),
            system,
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_deploy(&self, request: DeploySignedReq) -> anyhow::Result<DeployId> {
        record_trace!(request);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client
            .deploy_signed_contract(request.contract)
            .await?;

        if let Some(system) = request.system {
            write_client.deploy_signed_contract(system).await?;
        }

        Ok(deploy_id)
    }
}
