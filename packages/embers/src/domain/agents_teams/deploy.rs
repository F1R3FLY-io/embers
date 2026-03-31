use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::errors::ReadNodeError;
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

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

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_deploy_contract(&self, request: DeployReq) -> anyhow::Result<DeployResp> {
        record_trace!(request);

        // Diagnostic: check if registry entry is still accessible before deploy
        let env_uri_str: &str = self.uri.as_ref();
        let probe_code =
            format!(r#"new ret, rl(`rho:registry:lookup`) in {{ rl!(`{env_uri_str}`, *ret) }}"#,);
        match self
            .read_client
            .get_data::<serde_json::Value>(probe_code)
            .await
        {
            Ok(value) => tracing::info!(
                value = %value,
                "pre-deploy registry probe: env entry accessible"
            ),
            Err(ReadNodeError::ReturnValueMissing) => tracing::error!(
                env_uri = %env_uri_str,
                "pre-deploy registry probe: env entry NOT FOUND — registry lookup returned empty"
            ),
            Err(err) => tracing::error!(
                error = %err,
                "pre-deploy registry probe: failed"
            ),
        }

        // Control probe: check a well-known system URI
        let system_probe_code =
            r#"new ret, rl(`rho:registry:lookup`) in { rl!(`rho:lang:treeHashMap`, *ret) }"#
                .to_string();
        match self
            .read_client
            .get_data::<serde_json::Value>(system_probe_code)
            .await
        {
            Ok(value) => tracing::info!(
                value = %value,
                "pre-deploy registry probe: system URI (treeHashMap) accessible"
            ),
            Err(ReadNodeError::ReturnValueMissing) => tracing::error!(
                "pre-deploy registry probe: system URI (treeHashMap) NOT FOUND — entire registry may be broken"
            ),
            Err(err) => tracing::error!(
                error = %err,
                "pre-deploy registry probe: system URI check failed"
            ),
        }

        // Diagnostic probe: test whether the contract handler actually fires.
        // "list" should return data (possibly empty list) if the persistent
        // continuation for @agentsTeams is still reachable. If this returns
        // empty expr, the handler continuation is lost in the trie.
        let handler_probe_code = format!(
            r#"new ret, rl(`rho:registry:lookup`), agentsTeamsCh in {{
    rl!(`{env_uri_str}`, *agentsTeamsCh) |
    for(@(_, agentsTeams) <- agentsTeamsCh) {{
        @agentsTeams!("list", "diag_probe_address", *ret)
    }}
}}"#,
        );
        match self
            .read_client
            .get_data::<serde_json::Value>(handler_probe_code)
            .await
        {
            Ok(value) => tracing::info!(
                value = %value,
                "pre-deploy handler probe: contract handler FIRES — 'list' returned data"
            ),
            Err(ReadNodeError::ReturnValueMissing) => tracing::error!(
                env_uri = %env_uri_str,
                "pre-deploy handler probe: contract handler BLOCKED — 'list' returned empty expr. \
                 Persistent continuation for @agentsTeams may be lost in the trie."
            ),
            Err(err) => tracing::error!(
                error = %err,
                "pre-deploy handler probe: failed"
            ),
        }

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        let (graph, phlo_limit, deploy, system) = match request {
            DeployReq::AgentsTeam {
                id,
                version,
                address,
                phlo_limit,
                deploy,
            } => {
                let agents_team = self
                    .get_with_retry(
                        address,
                        id.clone(),
                        version.clone(),
                        30,
                        Duration::from_secs(1),
                    )
                    .await?
                    .context("agents team not found")?;
                let graph = agents_team.graph.context("agents team has no graph")?;

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
                            .valid_after_block_number(valid_after)
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

        Ok(DeployResp {
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aes_gcm::{Aes256Gcm, Key};
    use dashmap::DashMap;
    use firefly_client::models::SignedCode;

    use super::*;
    use crate::domain::agents_teams::AgentsTeamsService;
    use crate::domain::agents_teams::models::DeploySignedReq;
    use crate::domain::test_helpers::*;

    /// Build a minimal `AgentsTeamsService` wired to mock backends.
    fn make_service(
        read: MockReadNode,
        write: MockWriteNode,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsTeamsService {
            uri: test_uri(),
            write_client: write,
            read_client: read,
            observer_node_events: MockNodeEventSource::new(),
            aes_encryption_key: *Key::<Aes256Gcm>::from_slice(&[42u8; 32]),
            firesky_accounts: Arc::new(DashMap::new()),
        }
    }

    fn dummy_signed_code(tag: u8) -> SignedCode {
        SignedCode {
            contract: vec![tag; 16],
            sig: vec![0; 64],
            sig_algorithm: "secp256k1".into(),
            deployer: vec![0; 65],
        }
    }

    // -------------------------------------------------------
    // deploy_signed_deploy tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_deploy_signed_forwards_to_write_client() {
        let write = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(test_deploy_id()));
        let read = MockReadNode::new();

        let service = make_service(read, write.clone());

        let contract = dummy_signed_code(0xAA);
        let request = DeploySignedReq {
            contract: contract.clone(),
            system: None,
        };

        let deploy_id = service
            .deploy_signed_deploy(request)
            .await
            .expect("deploy_signed_deploy should succeed");

        assert_eq!(deploy_id, test_deploy_id());

        // Exactly one contract should have been deployed
        let deployed = write.deployed_contracts();
        assert_eq!(deployed.len(), 1, "expected exactly 1 deployed contract");
        assert_eq!(
            deployed[0],
            vec![0xAA; 16],
            "deployed contract bytes should match the main contract"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_deploys_both_when_system_present() {
        let main_id = DeployId::from("main-deploy-id".to_owned());
        let system_id = DeployId::from("system-deploy-id".to_owned());

        let write = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(main_id.clone()))
            .with_deploy_response(Ok(system_id));
        let read = MockReadNode::new();

        let service = make_service(read, write.clone());

        let main_contract = dummy_signed_code(0xBB);
        let system_contract = dummy_signed_code(0xCC);

        let request = DeploySignedReq {
            contract: main_contract.clone(),
            system: Some(system_contract.clone()),
        };

        let deploy_id = service
            .deploy_signed_deploy(request)
            .await
            .expect("deploy_signed_deploy should succeed");

        // The returned deploy ID should be from the first (main) deploy
        assert_eq!(deploy_id, main_id);

        // Both contracts should have been deployed in order
        let deployed = write.deployed_contracts();
        assert_eq!(deployed.len(), 2, "expected 2 deployed contracts");
        assert_eq!(
            deployed[0],
            vec![0xBB; 16],
            "first deployed contract should be the main contract"
        );
        assert_eq!(
            deployed[1],
            vec![0xCC; 16],
            "second deployed contract should be the system contract"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_propagates_write_error() {
        let write = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Err(anyhow::anyhow!("node unavailable")));
        let read = MockReadNode::new();

        let service = make_service(read, write);

        let request = DeploySignedReq {
            contract: dummy_signed_code(0x01),
            system: None,
        };

        let result = service.deploy_signed_deploy(request).await;
        assert!(result.is_err(), "should propagate write client error");
        assert!(
            result.unwrap_err().to_string().contains("node unavailable"),
            "error message should contain the original cause"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_system_error_propagates() {
        // Main deploy succeeds but system deploy fails
        let write = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(test_deploy_id()))
            .with_deploy_response(Err(anyhow::anyhow!("system deploy failed")));
        let read = MockReadNode::new();

        let service = make_service(read, write);

        let request = DeploySignedReq {
            contract: dummy_signed_code(0x01),
            system: Some(dummy_signed_code(0x02)),
        };

        let result = service.deploy_signed_deploy(request).await;
        assert!(result.is_err(), "should propagate system deploy error");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("system deploy failed"),
            "error message should contain the system deploy failure cause"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_no_system_does_not_deploy_extra() {
        // Enqueue two responses but only one should be consumed
        let write = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(test_deploy_id()));
        let read = MockReadNode::new();

        let service = make_service(read, write.clone());

        let request = DeploySignedReq {
            contract: dummy_signed_code(0xDD),
            system: None,
        };

        service
            .deploy_signed_deploy(request)
            .await
            .expect("deploy should succeed");

        let deployed = write.deployed_contracts();
        assert_eq!(
            deployed.len(),
            1,
            "with system=None, only the main contract should be deployed"
        );
    }
}
