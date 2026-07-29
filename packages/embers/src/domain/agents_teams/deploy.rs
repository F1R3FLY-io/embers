use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::errors::ReadNodeError;
use firefly_client::models::{DeployId, Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::future::try_join_all;

use crate::domain::agents::AgentsService;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::compilation::{AgentCodes, Code, parse, render};
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
    pub async fn prepare_deploy_contract(
        &self,
        request: DeployReq,
        agents: &AgentsService<R, W, N>,
    ) -> anyhow::Result<DeployResp> {
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
        let (graph, phlo_limit, deploy, system, address) = match request {
            DeployReq::AgentsTeam {
                id,
                version,
                address,
                phlo_limit,
                deploy,
            } => {
                let agents_team = self
                    .get_with_retry(
                        address.clone(),
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
                    Some(address),
                )
            }
            DeployReq::Graph {
                graph,
                phlo_limit,
                deploy,
            } => (graph, phlo_limit, deploy, None, None),
        };

        let timestamp = deploy.timestamp;

        let code = parse(&graph)?;
        let agent_codes = self
            .resolve_agent_codes(&code, address.as_ref(), agents)
            .await?;
        let code = render(code, deploy, &agent_codes)?;

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

    /// Reads the Rholang of every agent this team references, so the renderer
    /// can splice it in. Codes are read fresh on each deploy, which is what
    /// makes editing an agent and redeploying the team pick up the new version.
    #[tracing::instrument(level = "info", skip_all, err(Debug))]
    async fn resolve_agent_codes(
        &self,
        code: &Code<'_>,
        address: Option<&WalletAddress>,
        agents: &AgentsService<R, W, N>,
    ) -> anyhow::Result<AgentCodes> {
        let refs = code.agent_refs();
        if refs.is_empty() {
            return Ok(AgentCodes::default());
        }

        let address = address.context(
            "this graph uses agent nodes, so it must be deployed by agents team id and version; \
             the raw graph deploy path carries no wallet address to resolve agents against",
        )?;

        let fetched = try_join_all(refs.into_iter().map(|agent_ref| async move {
            // Reads that may trail a recent write go through the retrying
            // variant: the observer lags the validator briefly after finalization.
            let agent = agents
                .get_with_retry(
                    address.clone(),
                    agent_ref.id.clone(),
                    agent_ref.version.clone(),
                    30,
                    Duration::from_secs(1),
                )
                .await?
                .with_context(|| {
                    format!(
                        "agent {} version {} referenced by this team was not found for wallet {}",
                        agent_ref.id,
                        agent_ref.version,
                        AsRef::<String>::as_ref(address),
                    )
                })?;

            let source = agent.code.with_context(|| {
                format!(
                    "agent {} version {} referenced by this team has no code",
                    agent_ref.id, agent_ref.version,
                )
            })?;

            // Blank code would render a syntactically invalid contract, which
            // fails on chain with an opaque parse error, so reject it here.
            anyhow::ensure!(
                !source.trim().is_empty(),
                "agent {} version {} referenced by this team has empty code",
                agent_ref.id,
                agent_ref.version,
            );

            anyhow::Ok((agent_ref, source))
        }))
        .await?;

        Ok(fetched.into_iter().collect())
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
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use aes_gcm::{Aes256Gcm, Key};
    use dashmap::DashMap;
    use firefly_client::models::SignedCode;
    use serde_json::json;

    use super::*;
    use crate::domain::agents_teams::AgentsTeamsService;
    use crate::domain::agents_teams::compilation::{AgentRef, Node, Vertex};
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

    /// An `AgentsService` sharing the mock backends, so agent reads can be
    /// scripted independently of the team reads.
    fn make_agents_service(
        read: MockReadNode,
        write: MockWriteNode,
    ) -> AgentsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsService {
            uri: test_uri(),
            write_client: write,
            read_client: read,
            observer_node_events: MockNodeEventSource::new(),
        }
    }

    fn agent_ref() -> AgentRef {
        AgentRef {
            id: "agent-id".to_owned(),
            version: "agent-version".to_owned(),
        }
    }

    /// A `Code` holding a single agent node, enough to drive code resolution.
    fn agent_only_code() -> Code<'static> {
        Code {
            nodes: BTreeMap::from([(
                Vertex::from("aAgent"),
                Node::Agent {
                    agent_id: "agent-id".to_owned(),
                    agent_version: "agent-version".to_owned(),
                    agent_name: "Summarize".to_owned(),
                    from: vec![Vertex::from("aInput")],
                    output: true,
                },
            )]),
            output: None,
        }
    }

    /// Two distinct nodes pointing at the same agent version.
    fn code_using_one_agent_twice() -> Code<'static> {
        Code {
            nodes: BTreeMap::from([
                (
                    Vertex::from("aFirst"),
                    Node::Agent {
                        agent_id: "agent-id".to_owned(),
                        agent_version: "agent-version".to_owned(),
                        agent_name: "Summarize".to_owned(),
                        from: vec![Vertex::from("aInput")],
                        output: true,
                    },
                ),
                (
                    Vertex::from("aSecond"),
                    Node::Agent {
                        agent_id: "agent-id".to_owned(),
                        agent_version: "agent-version".to_owned(),
                        agent_name: "Summarize".to_owned(),
                        from: vec![Vertex::from("aFirst")],
                        output: true,
                    },
                ),
            ]),
            output: None,
        }
    }

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

    fn test_wallet_address() -> WalletAddress {
        WalletAddress::from(test_public_key())
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

    // -------------------------------------------------------
    // agent code resolution tests
    // -------------------------------------------------------

    const AGENT_SOURCE: &str = "contract main(@input, return) = { return!(input) }";

    #[tokio::test]
    async fn resolves_code_for_a_referenced_agent() {
        let read = MockReadNode::new().on_code_containing("\"get\"", agent_json(json!(AGENT_SOURCE)));
        let agents = make_agents_service(read, MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let codes = service
            .resolve_agent_codes(
                &agent_only_code(),
                Some(&test_wallet_address()),
                &agents,
            )
            .await
            .expect("resolving a present agent should succeed");

        assert_eq!(codes.get(&agent_ref()).map(String::as_str), Some(AGENT_SOURCE));
    }

    #[tokio::test]
    async fn reports_a_missing_agent() {
        // No scripted response: every read comes back empty.
        let agents = make_agents_service(MockReadNode::new(), MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let error = service
            .resolve_agent_codes(&agent_only_code(), Some(&test_wallet_address()), &agents)
            .await
            .expect_err("a team referencing a missing agent should not deploy");

        let message = format!("{error:#}");
        assert!(
            message.contains("agent-id") && message.contains("was not found"),
            "the error should name the missing agent, got: {message}"
        );
    }

    #[tokio::test]
    async fn rejects_an_agent_without_code() {
        let read = MockReadNode::new().on_code_containing("\"get\"", agent_json(json!(null)));
        let agents = make_agents_service(read, MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let error = service
            .resolve_agent_codes(&agent_only_code(), Some(&test_wallet_address()), &agents)
            .await
            .expect_err("an agent with no code should not deploy");

        assert!(
            format!("{error:#}").contains("has no code"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn rejects_blank_agent_code() {
        // Blank code renders a syntactically invalid contract, so it has to be
        // caught here rather than failing opaquely on chain.
        let read = MockReadNode::new().on_code_containing("\"get\"", agent_json(json!("   \n  ")));
        let agents = make_agents_service(read, MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let error = service
            .resolve_agent_codes(&agent_only_code(), Some(&test_wallet_address()), &agents)
            .await
            .expect_err("an agent with blank code should not deploy");

        assert!(
            format!("{error:#}").contains("empty code"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn reads_a_repeated_agent_only_once() {
        let read = MockReadNode::new().on_code_containing("\"get\"", agent_json(json!(AGENT_SOURCE)));
        let agents = make_agents_service(read.clone(), MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let codes = service
            .resolve_agent_codes(
                &code_using_one_agent_twice(),
                Some(&test_wallet_address()),
                &agents,
            )
            .await
            .expect("resolving a repeated agent should succeed");

        assert_eq!(codes.len(), 1, "the same agent version should collapse to one entry");
        assert_eq!(
            read.calls().len(),
            1,
            "the same agent version should be read once, not once per node"
        );
    }

    #[tokio::test]
    async fn a_graph_without_agents_reads_nothing() {
        let read = MockReadNode::new();
        let agents = make_agents_service(read.clone(), MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let empty = Code {
            nodes: BTreeMap::new(),
            output: None,
        };

        let codes = service
            .resolve_agent_codes(&empty, None, &agents)
            .await
            .expect("a team with no agents needs no address and no reads");

        assert!(codes.is_empty());
        assert!(read.calls().is_empty(), "no agent reads should be issued");
    }

    #[tokio::test]
    async fn agent_nodes_require_a_wallet_address() {
        let agents = make_agents_service(MockReadNode::new(), MockWriteNode::new());
        let service = make_service(MockReadNode::new(), MockWriteNode::new());

        let error = service
            .resolve_agent_codes(&agent_only_code(), None, &agents)
            .await
            .expect_err("agent nodes cannot be resolved without a wallet");

        assert!(
            format!("{error:#}").contains("wallet address"),
            "unexpected error: {error:#}"
        );
    }
}
