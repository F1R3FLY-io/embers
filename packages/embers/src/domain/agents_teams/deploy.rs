use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use firefly_client::errors::ReadNodeError;
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
