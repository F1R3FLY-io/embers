use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployData, DeployId, Uri, ValidAfter};
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
                for attempt in 1..=self.observer_sync.max_attempts {
                    match self.get(address.clone(), id.clone(), version.clone()).await {
                        Ok(Some(team)) => { agents_team = Some(team); break; }
                        _ => if attempt < self.observer_sync.max_attempts {
                            tokio::time::sleep(self.observer_sync.interval).await;
                        }
                    }
                }
                let agents_team = agents_team.context("agents team not visible on observer")?;
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

        if request.system.is_some() {
            // Wait for main deploy to finalize so recordDeploy executes
            // against state that includes the deployed graph.
            let mut finalized_block = 0u64;
            let deadline = tokio::time::Instant::now() + self.observer_sync.finalization_timeout;
            loop {
                if let Ok(Some(info)) = self.read_client.find_deploy_info(&deploy_id).await {
                    if self.read_client.is_finalized(&info.block_hash).await.unwrap_or(false) {
                        finalized_block = info.block_number;
                        break;
                    }
                }
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!("main deploy not finalized, skipping recordDeploy");
                    return Ok(deploy_id);
                }
                tokio::time::sleep(self.observer_sync.interval).await;
            }

            // Extract Rholang code from the pre-signed system contract and
            // re-deploy with correct valid_after using the service key.
            let system = request.system.unwrap();
            let proto = prost::Message::decode(system.contract.as_slice())
                .map(|msg: firefly_client::models::casper::DeployDataProto| msg.term);

            if let Ok(code) = proto {
                let deploy_data = DeployData::builder(code)
                    .valid_after_block_number(ValidAfter::Index(finalized_block))
                    .build();
                write_client.deploy(&self.service_key, deploy_data).await?;
            } else {
                // Fallback: submit pre-signed contract as-is
                write_client.deploy_signed_contract(system).await?;
            }
        }

        Ok(deploy_id)
    }
}
