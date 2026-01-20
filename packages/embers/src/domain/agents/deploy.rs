use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;

use crate::domain::agents::AgentsService;
use crate::domain::agents::models::{DeployReq, DeployResp, DeploySignedReq};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "ai_agents/record_deploy.rho")]
struct UpdateLastDeploy {
    env_uri: Uri,
    id: String,
    version: String,
    last_deploy: DateTime<Utc>,
}

impl AgentsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_deploy_contract(&self, request: DeployReq) -> anyhow::Result<DeployResp> {
        record_trace!(request);

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        let (code, phlo_limit, system) = match request {
            DeployReq::Agent {
                id,
                version,
                address,
                phlo_limit,
            } => {
                let code = self
                    .get(address, id.clone(), version.clone())
                    .await?
                    .context("agent not found")?
                    .code
                    .context("agent has no code")?;

                let system_code = UpdateLastDeploy {
                    env_uri: self.uri.clone(),
                    id,
                    version,
                    last_deploy: Utc::now(),
                }
                .render()?;

                (
                    code,
                    phlo_limit,
                    Some(
                        prepare_for_signing()
                            .code(system_code)
                            .valid_after_block_number(valid_after)
                            .call(),
                    ),
                )
            }
            DeployReq::Code { code, phlo_limit } => (code, phlo_limit, None),
        };

        Ok(DeployResp {
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
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

        write_client.propose().await?;
        Ok(deploy_id)
    }
}
