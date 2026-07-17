use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents::AgentsService;
use crate::domain::agents::models::{DeployReq, DeployResp, DeploySignedReq};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents/record_deploy.rho")]
struct UpdateLastDeploy {
    env_uri: Uri,
    id: String,
    version: String,
    last_deploy: DateTime<Utc>,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsService<R, W, N> {
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
                let agent = self
                    .get_with_retry(
                        address,
                        id.clone(),
                        version.clone(),
                        30,
                        Duration::from_secs(1),
                    )
                    .await?
                    .context("agent not found")?;
                let code = agent.code.context("agent has no code")?;

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

        Ok(deploy_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_helpers::*;

    fn make_service() -> AgentsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsService {
            uri: test_uri(),
            write_client: MockWriteNode::new()
                .with_deploy_response(Ok(test_deploy_id()))
                .with_deploy_response(Ok(test_deploy_id())),
            read_client: MockReadNode::new(),
            observer_node_events: MockNodeEventSource::new(),
        }
    }

    #[tokio::test]
    async fn test_deploy_signed_deploys_both() {
        let service = make_service();
        let request = DeploySignedReq {
            contract: test_signed_code(),
            system: Some(test_signed_code()),
        };

        let result = service.deploy_signed_deploy(request).await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");

        let deployed = service.write_client.deployed_contracts();
        assert_eq!(
            deployed.len(),
            2,
            "expected both contract and system to be deployed, but got {} deploys",
            deployed.len()
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_main_only() {
        let service = make_service();
        let request = DeploySignedReq {
            contract: test_signed_code(),
            system: None,
        };

        let result = service.deploy_signed_deploy(request).await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");

        let deployed = service.write_client.deployed_contracts();
        assert_eq!(
            deployed.len(),
            1,
            "expected only the main contract to be deployed, but got {} deploys",
            deployed.len()
        );
    }
}
