use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::{Inline, Render};
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

#[derive(Debug, Clone, Render)]
#[template(path = "agents/deploy_agent.rho")]
struct DeployAgentCode {
    /// The agent's source, emitted raw rather than as a Rholang string literal.
    code: Inline,
    /// Public name the deployed agent answers on.
    handle: String,
}

/// Name a standalone-deployed agent is reachable at. Derived from id and
/// version so redeploying a version is idempotent and two versions of the same
/// agent never collide.
fn agent_handle(id: &str, version: &str) -> String {
    format!("agent:{id}:{version}")
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
                let source = agent.code.context("agent has no code")?;
                anyhow::ensure!(
                    !source.trim().is_empty(),
                    "agent {id} version {version} has empty code",
                );

                // Bind the agent's entry contract and publish it: deployed
                // verbatim, an agent written for a team is rejected outright
                // because its `main` is a free name at the top level.
                let code = DeployAgentCode {
                    code: Inline::from(source),
                    handle: agent_handle(&id, &version),
                }
                .render()?;

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

#[cfg(test)]
mod deploy_wrapping_tests {
    use serde_json::json;

    use firefly_client::models::WalletAddress;

    use super::*;
    use crate::domain::agents::models::DeployReq;
    use crate::domain::test_helpers::*;

    const AGENT_SOURCE: &str = "contract main(@input, return) = { return!(input) }";

    fn agent_json(code: serde_json::Value) -> serde_json::Value {
        json!({
            "id": "agent-id",
            "version": "agent-version",
            "created_at": 0,
            "last_deploy": null,
            "name": "Summarize",
            "description": null,
            "shard": null,
            "logo": null,
            "code": code,
        })
    }

    fn service_returning(code: serde_json::Value) -> AgentsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsService {
            uri: test_uri(),
            write_client: MockWriteNode::new().with_head_block_index(10),
            read_client: MockReadNode::new().on_code_containing("\"get\"", agent_json(code)),
            observer_node_events: MockNodeEventSource::new(),
        }
    }

    fn agent_request() -> DeployReq {
        DeployReq::Agent {
            id: "agent-id".to_owned(),
            version: "agent-version".to_owned(),
            address: WalletAddress::from(test_public_key()),
            phlo_limit: 50_000i64.try_into().expect("positive phlo limit"),
        }
    }

    #[tokio::test]
    async fn binds_the_entry_contract_so_a_team_agent_can_deploy() {
        let service = service_returning(json!(AGENT_SOURCE));

        let response = service
            .prepare_deploy_contract(agent_request())
            .await
            .expect("deploying an agent written for a team should succeed");

        let code = String::from_utf8_lossy(&response.contract.0).into_owned();

        assert!(
            code.contains("new main in"),
            "`main` must be bound, otherwise the node rejects it as a free name:\n{code}"
        );
        assert!(
            code.contains(AGENT_SOURCE),
            "the agent's own source should be spliced in verbatim:\n{code}"
        );
        assert!(
            code.contains(r#"contract @"agent:agent-id:agent-version""#),
            "the deployed agent should be reachable at a handle:\n{code}"
        );
        assert!(
            !code.contains(r#""contract main"#),
            "the source must be raw, never a quoted Rholang string:\n{code}"
        );
    }

    #[tokio::test]
    async fn rejects_blank_agent_code() {
        let service = service_returning(json!("   \n "));

        let error = service
            .prepare_deploy_contract(agent_request())
            .await
            .expect_err("an agent with blank code should not deploy");

        assert!(
            format!("{error:#}").contains("empty code"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn raw_code_deploys_are_left_alone() {
        // The raw-code path is arbitrary user Rholang, not an agent, so it must
        // not be wrapped.
        let service = service_returning(json!(AGENT_SOURCE));
        let raw = "new x in { x!(1) }".to_owned();

        let response = service
            .prepare_deploy_contract(DeployReq::Code {
                code: raw.clone(),
                phlo_limit: 50_000i64.try_into().expect("positive phlo limit"),
            })
            .await
            .expect("raw code should deploy");

        let code = String::from_utf8_lossy(&response.contract.0).into_owned();
        assert!(
            code.contains(&raw),
            "raw code must be deployed verbatim, got:\n{code}"
        );
        assert!(
            !code.contains("new main in"),
            "the raw-code path must not be wrapped:\n{code}"
        );
    }
}
