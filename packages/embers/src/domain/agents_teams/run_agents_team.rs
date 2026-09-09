use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::continuation_store::{ArtifactKind, ArtifactPointer};
use crate::domain::agents_teams::models::{DeploySignedRunReq, RunReq, RunResp};
use crate::domain::agents_teams::runtime::ContinuationEnvelope;
use crate::domain::agents_teams::workflow::WorkflowDefinition;
use crate::domain::common::{prepare_for_signing, record_trace, utc_now_protocol};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/run.rho")]
struct RunAgentsTeam {
    env_uri: Uri,
    agents_team: Uri,
    prompt: String,
    run_id: uuid::Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run_result.rho")]
struct GetAgentsTeamResult {
    deploy_id: DeployId,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_manifest.rho")]
struct GetAgentsTeamManifest {
    agents_team: Uri,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeamManifest {
    #[serde(default)]
    pub deployment_bindings: std::collections::BTreeMap<String, String>,
    #[serde(deserialize_with = "crate::blockchain::common::deserialize_unescaped")]
    pub definition: String,
    pub definition_hash: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/begin_durable_run.rho")]
struct BeginDurableRun {
    env_uri: Uri,
    agents_team: Uri,
    run_id: uuid::Uuid,
    created_at: DateTime<Utc>,
    definition_hash: String,
    request_content_hash: String,
    request_capability_hash: String,
    continuation_content_hash: String,
    continuation_capability_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableRunRequest {
    pub schema_version: String,
    pub run_id: String,
    pub agents_team: String,
    pub prompt: String,
    pub created_at: DateTime<Utc>,
    /// Present exactly on grant-initiated runs (schema v3); plain runs keep
    /// writing v2 without it, and v2 artifacts deserialize with `None` here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_context: Option<crate::domain::agents_teams::models::ChannelContext>,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_agents_team_contract(
        &self,
        request: RunReq,
    ) -> anyhow::Result<RunResp> {
        record_trace!(request);

        let run_id = uuid::Uuid::now_v7();
        let created_at = utc_now_protocol();
        let run_id_text = run_id.to_string();
        let durable_request = DurableRunRequest {
            schema_version: "embers-agent-team-run-request-v2".to_owned(),
            run_id: run_id_text.clone(),
            agents_team: request.agents_team.as_ref().to_owned(),
            prompt: request.prompt.clone(),
            created_at,
            channel_context: None,
        };
        let request_pointer = self.continuation_store.put(
            ArtifactKind::RunRequest,
            &run_id_text,
            "request",
            &durable_request,
        )?;

        let manifest = self.get_team_manifest(request.agents_team.clone()).await?;
        let (contract, definition_hash, continuation_pointer) = if let Some(manifest) = manifest {
            anyhow::ensure!(
                manifest.protocol == "embers-agent-team-control-v1",
                "unsupported Agent Team control protocol {}",
                manifest.protocol
            );
            let definition = WorkflowDefinition::from_json(manifest.definition)?;
            definition.parsed().validate_publishable()?;
            anyhow::ensure!(
                definition.definition_hash() == manifest.definition_hash,
                "Agent Team manifest definition hash mismatch"
            );
            let continuation = ContinuationEnvelope::start(
                definition.parsed(),
                run_id_text.clone(),
                request.agents_team.as_ref().to_owned(),
                manifest.definition_hash.clone(),
                Value::String(request.prompt.clone()),
            )?;
            let pointer = self.continuation_store.put(
                ArtifactKind::Continuation,
                &run_id_text,
                "revision-0",
                &continuation,
            )?;
            let contract = BeginDurableRun {
                env_uri: self.uri.clone(),
                agents_team: request.agents_team.clone(),
                run_id,
                created_at,
                definition_hash: manifest.definition_hash.clone(),
                request_content_hash: request_pointer.content_hash.clone(),
                request_capability_hash: request_pointer.capability_hash.clone(),
                continuation_content_hash: pointer.content_hash.clone(),
                continuation_capability_hash: pointer.capability_hash.clone(),
            }
            .render()?;
            (contract, Some(manifest.definition_hash), Some(pointer))
        } else {
            let contract = RunAgentsTeam {
                env_uri: self.uri.clone(),
                agents_team: request.agents_team.clone(),
                prompt: request.prompt.clone(),
                run_id,
                created_at,
            }
            .render()?;
            (contract, None, None)
        };

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RunResp {
            run_id: run_id_text,
            contract: prepare_for_signing()
                .code(contract)
                .valid_after_block_number(valid_after)
                .call(),
            created_at,
            durable: definition_hash.is_some(),
            definition_hash,
            request_content_hash: request_pointer.content_hash,
            request_capability_hash: request_pointer.capability_hash,
            continuation_content_hash: continuation_pointer
                .as_ref()
                .map(|pointer| pointer.content_hash.clone()),
            continuation_capability_hash: continuation_pointer
                .map(|pointer| pointer.capability_hash),
        })
    }

    pub(crate) async fn get_team_manifest(
        &self,
        agents_team: Uri,
    ) -> anyhow::Result<Option<TeamManifest>> {
        let code = GetAgentsTeamManifest { agents_team }.render()?;
        self.read_client
            .get_data_or_none(code)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_run_agents_team(
        &self,
        request: DeploySignedRunReq,
    ) -> anyhow::Result<serde_json::Value> {
        record_trace!(request);

        anyhow::ensure!(
            request.prepare_response.run_id.len() <= 128,
            "run id exceeds 128 bytes"
        );
        anyhow::ensure!(
            request.prepare_response.durable == request.prepare_response.definition_hash.is_some(),
            "prepared run durability metadata is inconsistent"
        );

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client
            .deploy_signed_contract(request.contract)
            .await?;

        let result = self
            .observer_node_events
            .wait_for_deploy(&deploy_id, self.deploy_finalization_timeout)
            .await;

        match result {
            Some(true) => return Err(anyhow!("deploy {deploy_id} errored on chain")),
            None => return Err(anyhow!("block is not finalized")),
            Some(false) => {}
        }

        if !request.prepare_response.durable {
            let code = GetAgentsTeamResult { deploy_id }.render()?;
            return self
                .read_client
                .get_data_with_retry(code, 5, Duration::from_millis(500))
                .await
                .map_err(Into::into);
        }

        let run_id = request.prepare_response.run_id.clone();
        let lock = self
            .run_locks
            .entry(run_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let registered = self
            .wait_for_run_state(
                &request.prepare_response.run_id,
                "signed run registration",
                |state| {
                    verify_durable_run_registration(
                        state,
                        &request.prepare_request.agents_team,
                        &request.prepare_response,
                    )
                    .is_ok()
                },
            )
            .await?;
        verify_durable_run_registration(
            &registered,
            &request.prepare_request.agents_team,
            &request.prepare_response,
        )?;
        self.resume_durable_run(request.prepare_request, request.prepare_response)
            .await
    }

    pub(crate) fn locate_checked_artifact(
        &self,
        kind: ArtifactKind,
        run_id: &str,
        object_id: &str,
        content_hash: &str,
        capability_hash: &str,
    ) -> anyhow::Result<ArtifactPointer> {
        let pointer = self
            .continuation_store
            .locate(kind, run_id, object_id)?
            .with_context(|| format!("{kind:?} artifact {object_id} for run {run_id} is missing"))?;
        anyhow::ensure!(
            pointer.content_hash == content_hash,
            "artifact content hash differs from the user-authorized run metadata"
        );
        anyhow::ensure!(
            pointer.capability_hash == capability_hash,
            "artifact capability hash differs from the user-authorized run metadata"
        );
        Ok(pointer)
    }
}

fn verify_durable_run_registration(
    state: &crate::domain::agents_teams::models::RunState,
    agents_team: &Uri,
    prepared: &RunResp,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.run_id == prepared.run_id,
        "on-chain run id differs from the signed request"
    );
    anyhow::ensure!(
        &state.agents_team == agents_team,
        "on-chain Agent Team differs from the signed request"
    );
    anyhow::ensure!(
        state.created_at == prepared.created_at,
        "on-chain creation time differs from the signed request"
    );
    anyhow::ensure!(
        state.revision == 0,
        "newly registered run must begin at revision zero"
    );
    anyhow::ensure!(
        state.status == "running",
        "newly registered run must begin in running state"
    );
    anyhow::ensure!(
        state.definition_hash == prepared.definition_hash,
        "on-chain definition hash differs from the signed request"
    );
    anyhow::ensure!(
        state.request_content_hash.as_ref() == Some(&prepared.request_content_hash),
        "on-chain request content hash differs from the signed request"
    );
    anyhow::ensure!(
        state.request_capability_hash.as_ref() == Some(&prepared.request_capability_hash),
        "on-chain request capability hash differs from the signed request"
    );
    anyhow::ensure!(
        state.continuation_content_hash == prepared.continuation_content_hash,
        "on-chain continuation content hash differs from the signed request"
    );
    anyhow::ensure!(
        state.continuation_capability_hash == prepared.continuation_capability_hash,
        "on-chain continuation capability hash differs from the signed request"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dashmap::DashMap;

    use super::*;
    use crate::domain::test_helpers::*;

    #[test]
    fn team_manifest_accepts_legacy_and_container_metadata() {
        let mut value = serde_json::json!({
            "definition": "{}",
            "definitionHash": "hash",
            "protocol": "embers-agent-team-control-v1"
        });
        let legacy: TeamManifest = serde_json::from_value(value.clone()).unwrap();
        assert!(legacy.deployment_bindings.is_empty());
        value["deploymentBindings"] = serde_json::json!({});
        let empty: TeamManifest = serde_json::from_value(value.clone()).unwrap();
        assert!(empty.deployment_bindings.is_empty());
        value["deploymentBindings"] = serde_json::json!({"agent": "rho:id:container"});
        let container: TeamManifest = serde_json::from_value(value).unwrap();
        assert_eq!(container.deployment_bindings["agent"], "rho:id:container");
        assert_eq!(container.definition_hash, legacy.definition_hash);
    }

    fn make_service(
        event_source: MockNodeEventSource,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new().with_deploy_response(Ok(test_deploy_id())),
            read_client: MockReadNode::new(),
            observer_node_events: event_source,
            service_key: test_secret_key(),
            aes_encryption_key: [42u8; 32].into(),
            continuation_store: crate::domain::agents_teams::continuation_store::F1r3driveContinuationStore::open_for_test(
                tempfile::tempdir().expect("temporary store").path(),
                [7u8; 32],
                [9u8; 32],
            )
            .expect("test continuation store"),
            deploy_finalization_timeout: Duration::from_secs(5),
            run_locks: Arc::new(DashMap::new()),
            firesky_accounts: Arc::new(DashMap::new()),
        }
    }

    fn legacy_run_request() -> DeploySignedRunReq {
        DeploySignedRunReq {
            contract: test_signed_code(),
            prepare_request: RunReq {
                prompt: "test".to_owned(),
                agents_team: test_uri(),
            },
            prepare_response: RunResp {
                run_id: uuid::Uuid::nil().to_string(),
                contract: crate::domain::common::PreparedContract(Vec::new()),
                created_at: DateTime::<Utc>::UNIX_EPOCH,
                durable: false,
                definition_hash: None,
                request_content_hash: String::new(),
                request_capability_hash: String::new(),
                continuation_content_hash: None,
                continuation_capability_hash: None,
            },
        }
    }

    fn durable_run_request() -> DeploySignedRunReq {
        let mut request = legacy_run_request();
        request.prepare_response.durable = true;
        request.prepare_response.definition_hash = Some("definition-hash".to_owned());
        request.prepare_response.request_content_hash = "request-content".to_owned();
        request.prepare_response.request_capability_hash = "request-capability".to_owned();
        request.prepare_response.continuation_content_hash =
            Some("continuation-content".to_owned());
        request.prepare_response.continuation_capability_hash =
            Some("continuation-capability".to_owned());
        request
    }

    #[test]
    fn run_template_orders_ledger_acknowledgements_around_team_work() {
        let code = RunAgentsTeam {
            env_uri: test_uri(),
            agents_team: test_uri(),
            prompt: "hello".to_owned(),
            run_id: uuid::Uuid::nil(),
            created_at: DateTime::<Utc>::UNIX_EPOCH,
        }
        .render()
        .expect("run template should render");

        assert!(code.contains("beginRun"));
        assert!(code.contains("for(_ <- runStarted)"));
        assert!(code.contains("finishRun"));
        assert!(code.contains("for(_ <- runFinished)"));
    }

    #[tokio::test]
    async fn test_deploy_signed_run_chain_error() {
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(true));
        let service = make_service(event_source);

        let result = service
            .deploy_signed_run_agents_team(legacy_run_request())
            .await;

        assert!(result.is_err(), "expected Err when deploy errored on chain");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("errored on chain"),
            "expected 'errored on chain' in error message, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_run_not_finalized() {
        let event_source = MockNodeEventSource::new().with_wait_result("test-deploy-id", None);
        let service = make_service(event_source);

        let result = service
            .deploy_signed_run_agents_team(legacy_run_request())
            .await;

        assert!(result.is_err(), "expected Err when block is not finalized");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not finalized"),
            "expected 'not finalized' in error message, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn finalized_durable_run_requires_observable_ledger_registration() {
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(false));
        let service = make_service(event_source);

        let error = service
            .deploy_signed_run_agents_team(durable_run_request())
            .await
            .expect_err("workflow effects must not execute without a ledger record");

        assert!(
            error.to_string().contains("signed run registration"),
            "unexpected error: {error:#}"
        );
    }
}
