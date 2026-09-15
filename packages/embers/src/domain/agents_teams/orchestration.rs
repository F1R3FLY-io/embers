use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, bail, ensure};
use chrono::{DateTime, Utc};
use firefly_client::helpers::sign_prepared_contract;
use firefly_client::models::{DeployData, DeployId, SignedCode, Uri};
use firefly_client::rendering::{Render, Value as RhoValue};
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use secp256k1::{Message, Secp256k1};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::AgentsTeamsService;
use super::continuation_store::{ArtifactKind, ArtifactPointer};
use super::models::{RunReq, RunResp, RunState};
use super::run_agents_team::DurableRunRequest;
use super::runtime::{
    ActivityObservation,
    ContinuationEnvelope,
    EffectRequest,
    GuardRequest,
    KernelOutcome,
    PendingWait,
    hash_json,
};
use super::workflow::{ActivityReference, WorkflowDefinition};
use crate::domain::common::prepare_for_signing;

const CONTROL_PROTOCOL: &str = "embers-agent-team-control-v1";
const MAX_NESTED_WORKFLOW_DEPTH: usize = 32;

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/invoke_activity.rho")]
struct InvokeActivity {
    agents_team: Uri,
    node_id: String,
    input: RhoValue,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/invoke_guard.rho")]
struct InvokeGuard {
    agents_team: Uri,
    transition_id: String,
    value: RhoValue,
    input: RhoValue,
    state: RhoValue,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run_result.rho")]
struct GetDeployResult {
    deploy_id: DeployId,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/advance_run.rho")]
struct AdvanceRun {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    status: String,
    definition_hash: String,
    continuation_content_hash: String,
    continuation_capability_hash: String,
    wait_ids: Vec<String>,
    due_at: Option<DateTime<Utc>>,
    approval_authority_grants: Vec<String>,
    output_content_hash: Option<String>,
    output_capability_hash: Option<String>,
    event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EffectOutbox {
    schema_version: String,
    operation_id: String,
    signed_deploy: SignedCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GuardObservation {
    schema_version: String,
    key: String,
    value: bool,
    deploy_id: String,
    observed_at: DateTime<Utc>,
    signature: String,
}

#[derive(Debug, Clone)]
struct AdvanceMetadata<'a> {
    status: &'a str,
    event_type: &'a str,
    waits: &'a [PendingWait],
    output: Option<&'a ArtifactPointer>,
}

struct ExpectedRunAdvancement<'a> {
    revision: u64,
    status: &'a str,
    definition_hash: &'a str,
    continuation: &'a ArtifactPointer,
    wait_ids: &'a [String],
    due_at: &'a Option<DateTime<Utc>>,
    approval_authority_grants: &'a [String],
    output_content_hash: &'a Option<String>,
    output_capability_hash: &'a Option<String>,
    event_type: &'a str,
}

impl ExpectedRunAdvancement<'_> {
    fn verify(&self, state: &RunState) -> anyhow::Result<()> {
        ensure!(
            state.revision == self.revision,
            "on-chain run revision did not advance"
        );
        ensure!(
            state.status == self.status,
            "on-chain run status differs from the finalized advancement"
        );
        ensure!(
            state.definition_hash.as_deref() == Some(self.definition_hash),
            "on-chain definition hash differs from the finalized advancement"
        );
        ensure!(
            state.continuation_content_hash.as_deref()
                == Some(self.continuation.content_hash.as_str()),
            "on-chain continuation content hash differs from the finalized advancement"
        );
        ensure!(
            state.continuation_capability_hash.as_deref()
                == Some(self.continuation.capability_hash.as_str()),
            "on-chain continuation capability hash differs from the finalized advancement"
        );
        ensure!(
            state.wait_ids == self.wait_ids,
            "on-chain waits differ from the finalized advancement"
        );
        ensure!(
            &state.due_at == self.due_at,
            "on-chain deadline differs from the finalized advancement"
        );
        ensure!(
            state.approval_authority_grants == self.approval_authority_grants,
            "on-chain approval grants differ from the finalized advancement"
        );
        ensure!(
            &state.output_content_hash == self.output_content_hash,
            "on-chain output content hash differs from the finalized advancement"
        );
        ensure!(
            &state.output_capability_hash == self.output_capability_hash,
            "on-chain output capability hash differs from the finalized advancement"
        );
        ensure!(
            state.command_id.is_none()
                && state.command_content_hash.is_none()
                && state.command_capability_hash.is_none()
                && state.command_actor.is_none()
                && state.command_class.is_none()
                && state.command_wait_id.is_none(),
            "finalized advancement did not clear the claimed command"
        );
        ensure!(
            state
                .events
                .last()
                .is_some_and(|event| event.event_type == self.event_type),
            "on-chain event log differs from the finalized advancement"
        );
        Ok(())
    }
}

enum Execution<T> {
    Observed(T),
    Terminal { code: &'static str, message: String },
}

enum NestedExecution {
    Observed(Box<ActivityObservation>),
    Suspended(Vec<PendingWait>),
    Terminal { code: &'static str, message: String },
}

enum SubmitOutcome {
    Finalized(DeployId),
    TimedOut(DeployId),
    Errored(DeployId),
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub(crate) async fn resume_durable_run(
        &self,
        request: RunReq,
        response: RunResp,
    ) -> anyhow::Result<Value> {
        let run_id = &response.run_id;
        let request_pointer = self.locate_checked_artifact(
            ArtifactKind::RunRequest,
            run_id,
            "request",
            &response.request_content_hash,
            &response.request_capability_hash,
        )?;
        let stored_request: DurableRunRequest =
            self.continuation_store
                .get(&request_pointer, run_id, "request")?;
        ensure!(
            stored_request.run_id == *run_id,
            "stored run request id mismatch"
        );
        ensure!(
            stored_request.agents_team.as_str() == request.agents_team.as_ref().as_str(),
            "stored run request team mismatch"
        );
        ensure!(
            stored_request.prompt == request.prompt,
            "stored prompt mismatch"
        );

        let manifest = self
            .get_team_manifest(request.agents_team.clone())
            .await?
            .context("durable Agent Team no longer exposes its control manifest")?;
        ensure!(
            manifest.protocol == CONTROL_PROTOCOL,
            "Agent Team control protocol changed"
        );
        let definition = WorkflowDefinition::from_json(manifest.definition)?;
        definition.parsed().validate_publishable()?;
        let expected_hash = response
            .definition_hash
            .as_deref()
            .context("durable run has no definition hash")?;
        ensure!(
            manifest.definition_hash == expected_hash,
            "Agent Team definition changed after prepare"
        );
        ensure!(
            definition.definition_hash() == expected_hash,
            "manifest definition hash mismatch"
        );

        let continuation_content_hash = response
            .continuation_content_hash
            .as_deref()
            .context("durable run has no initial continuation hash")?;
        let continuation_capability_hash = response
            .continuation_capability_hash
            .as_deref()
            .context("durable run has no initial continuation capability hash")?;
        let pointer = self.locate_checked_artifact(
            ArtifactKind::Continuation,
            run_id,
            "revision-0",
            continuation_content_hash,
            continuation_capability_hash,
        )?;
        let continuation: ContinuationEnvelope =
            self.continuation_store
                .get(&pointer, run_id, "revision-0")?;
        continuation.validate_identity(run_id, request.agents_team.as_ref(), expected_hash)?;

        self.drive_continuation(
            request.agents_team,
            definition,
            continuation,
            stored_request.created_at,
        )
        .await
    }

    pub(crate) async fn drive_continuation(
        &self,
        team_uri: Uri,
        definition: WorkflowDefinition,
        mut continuation: ContinuationEnvelope,
        mut logical_time: DateTime<Utc>,
    ) -> anyhow::Result<Value> {
        loop {
            let outcome = match continuation.run_until_blocked(definition.parsed(), logical_time) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return self
                        .fail_continuation(
                            &mut continuation,
                            "kernelRejectedState",
                            &error.to_string(),
                        )
                        .await;
                }
            };
            match outcome {
                KernelOutcome::Effects(effects) => {
                    for effect in effects {
                        self.trace_started(&continuation, &effect).await?;
                        let execution =
                            if matches!(&effect.activity, ActivityReference::Workflow { .. }) {
                                self.execute_nested_activity(
                                    &mut continuation,
                                    effect.clone(),
                                    logical_time,
                                    0,
                                )
                                .await?
                            } else {
                                match self.execute_activity(&team_uri, &effect).await? {
                                    Execution::Observed(observation) => {
                                        NestedExecution::Observed(Box::new(observation))
                                    }
                                    Execution::Terminal { code, message } => {
                                        NestedExecution::Terminal { code, message }
                                    }
                                }
                            };
                        let observation = match execution {
                            NestedExecution::Observed(observation) => *observation,
                            NestedExecution::Suspended(waits) => {
                                self.checkpoint_and_advance(
                                    &mut continuation,
                                    AdvanceMetadata {
                                        status: "suspended",
                                        event_type: "nestedRunSuspended",
                                        waits: &waits,
                                        output: None,
                                    },
                                )
                                .await?;
                                return Ok(json!({
                                    "disposition": "suspended",
                                    "runId": continuation.run_id,
                                    "revision": continuation.revision,
                                    "waits": waits,
                                }));
                            }
                            NestedExecution::Terminal { code, message } => {
                                self.trace_failed(&team_uri, &effect, &message).await?;
                                return self
                                    .fail_continuation(&mut continuation, code, &message)
                                    .await;
                            }
                        };
                        self.trace_finished(&team_uri, &effect, &observation)
                            .await?;
                        logical_time = observation.observed_at;
                        if let Err(error) =
                            continuation.apply_effect_observation(observation, logical_time)
                        {
                            return self
                                .fail_continuation(
                                    &mut continuation,
                                    "invalidActivityObservation",
                                    &error.to_string(),
                                )
                                .await;
                        }
                        self.checkpoint_and_advance(
                            &mut continuation,
                            AdvanceMetadata {
                                status: "running",
                                event_type: "effectObserved",
                                waits: &[],
                                output: None,
                            },
                        )
                        .await?;
                    }
                }
                KernelOutcome::Guards(guards) => {
                    for guard in guards {
                        let observation = match self.execute_guard(&team_uri, &guard).await? {
                            Execution::Observed(observation) => observation,
                            Execution::Terminal { code, message } => {
                                return self
                                    .fail_continuation(&mut continuation, code, &message)
                                    .await;
                            }
                        };
                        logical_time = observation.observed_at;
                        if let Err(error) =
                            continuation.apply_guard_observation(&guard, observation.value)
                        {
                            return self
                                .fail_continuation(
                                    &mut continuation,
                                    "invalidGuardObservation",
                                    &error.to_string(),
                                )
                                .await;
                        }
                        self.checkpoint_and_advance(
                            &mut continuation,
                            AdvanceMetadata {
                                status: "running",
                                event_type: "guardObserved",
                                waits: &[],
                                output: None,
                            },
                        )
                        .await?;
                    }
                }
                KernelOutcome::Suspended(waits) => {
                    self.checkpoint_and_advance(
                        &mut continuation,
                        AdvanceMetadata {
                            status: "suspended",
                            event_type: "runSuspended",
                            waits: &waits,
                            output: None,
                        },
                    )
                    .await?;
                    return Ok(json!({
                        "disposition": "suspended",
                        "runId": continuation.run_id,
                        "revision": continuation.revision,
                        "waits": waits,
                    }));
                }
                KernelOutcome::Completed(output) => {
                    let output_pointer = self.continuation_store.put(
                        ArtifactKind::Output,
                        &continuation.run_id,
                        "result",
                        &output,
                    )?;
                    self.checkpoint_and_advance(
                        &mut continuation,
                        AdvanceMetadata {
                            status: "completed",
                            event_type: "runCompleted",
                            waits: &[],
                            output: Some(&output_pointer),
                        },
                    )
                    .await?;
                    return Ok(output);
                }
                KernelOutcome::Cancelled => {
                    let output = json!({"disposition":"cancelled"});
                    let output_pointer = self.continuation_store.put(
                        ArtifactKind::Output,
                        &continuation.run_id,
                        "result",
                        &output,
                    )?;
                    self.checkpoint_and_advance(
                        &mut continuation,
                        AdvanceMetadata {
                            status: "cancelled",
                            event_type: "runCancelled",
                            waits: &[],
                            output: Some(&output_pointer),
                        },
                    )
                    .await?;
                    return Ok(output);
                }
            }
        }
    }

    fn execute_nested_activity<'a>(
        &'a self,
        parent: &'a mut ContinuationEnvelope,
        request: EffectRequest,
        mut logical_time: DateTime<Utc>,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NestedExecution>> + Send + 'a>> {
        Box::pin(async move {
            if depth >= MAX_NESTED_WORKFLOW_DEPTH {
                return Ok(NestedExecution::Terminal {
                    code: "nestedWorkflowDepthExceeded",
                    message: format!("nested workflow depth exceeds {MAX_NESTED_WORKFLOW_DEPTH}"),
                });
            }

            if let Some(pointer) = self.continuation_store.locate(
                ArtifactKind::Observation,
                &request.run_id,
                &request.effect_key,
            )? {
                parent.nested_workflows.remove(&request.effect_key);
                let observation =
                    self.continuation_store
                        .get(&pointer, &request.run_id, &request.effect_key)?;
                return Ok(NestedExecution::Observed(Box::new(observation)));
            }

            let ActivityReference::Workflow { team_id, version } = &request.activity else {
                return Ok(NestedExecution::Terminal {
                    code: "invalidNestedWorkflowReference",
                    message: "nested execution received a non-workflow activity".to_owned(),
                });
            };
            let child_uri = match Uri::try_from(team_id.clone()) {
                Ok(uri) => uri,
                Err(error) => {
                    return Ok(NestedExecution::Terminal {
                        code: "invalidNestedWorkflowUri",
                        message: error.to_string(),
                    });
                }
            };
            let Some(manifest) = self.get_team_manifest(child_uri.clone()).await? else {
                return Ok(NestedExecution::Terminal {
                    code: "nestedWorkflowManifestMissing",
                    message: format!("nested Agent Team {team_id} has no control manifest"),
                });
            };
            if manifest.protocol != CONTROL_PROTOCOL {
                return Ok(NestedExecution::Terminal {
                    code: "nestedWorkflowProtocolMismatch",
                    message: format!(
                        "nested Agent Team {team_id} uses unsupported protocol {}",
                        manifest.protocol
                    ),
                });
            }
            if manifest.definition_hash != *version {
                return Ok(NestedExecution::Terminal {
                    code: "nestedWorkflowVersionMismatch",
                    message: format!(
                        "nested Agent Team {team_id} resolved to {}, not pinned version {version}",
                        manifest.definition_hash
                    ),
                });
            }
            let definition = match WorkflowDefinition::from_json(manifest.definition) {
                Ok(definition) => definition,
                Err(error) => {
                    return Ok(NestedExecution::Terminal {
                        code: "invalidNestedWorkflowDefinition",
                        message: error.to_string(),
                    });
                }
            };
            if let Err(error) = definition.parsed().validate_publishable() {
                return Ok(NestedExecution::Terminal {
                    code: "invalidNestedWorkflowDefinition",
                    message: error.to_string(),
                });
            }
            if definition.definition_hash() != *version {
                return Ok(NestedExecution::Terminal {
                    code: "nestedWorkflowDefinitionHashMismatch",
                    message: format!("nested Agent Team {team_id} manifest content is not pinned"),
                });
            }

            if !parent.nested_workflows.contains_key(&request.effect_key) {
                let child = ContinuationEnvelope::start_nested(
                    definition.parsed(),
                    request.run_id.clone(),
                    request.effect_key.clone(),
                    team_id.clone(),
                    version.clone(),
                    request.input.clone(),
                )?;
                parent
                    .nested_workflows
                    .insert(request.effect_key.clone(), Box::new(child));
            }
            {
                let child = parent
                    .nested_workflows
                    .get(&request.effect_key)
                    .context("nested continuation disappeared")?;
                if let Err(error) = child.validate_identity(&request.run_id, team_id, version) {
                    return Ok(NestedExecution::Terminal {
                        code: "nestedWorkflowIdentityMismatch",
                        message: error.to_string(),
                    });
                }
                if child.execution_id != request.effect_key
                    || hash_json(&child.original_input) != request.input_hash
                {
                    return Ok(NestedExecution::Terminal {
                        code: "nestedWorkflowIdentityMismatch",
                        message: "nested continuation execution or input identity differs"
                            .to_owned(),
                    });
                }
            }

            let (output, disposition) = loop {
                let outcome = {
                    let child = parent
                        .nested_workflows
                        .get_mut(&request.effect_key)
                        .context("nested continuation disappeared")?;
                    match child.run_until_blocked(definition.parsed(), logical_time) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            return Ok(NestedExecution::Terminal {
                                code: "nestedWorkflowKernelRejectedState",
                                message: error.to_string(),
                            });
                        }
                    }
                };
                match outcome {
                    KernelOutcome::Effects(effects) => {
                        for effect in effects {
                            let child = parent
                                .nested_workflows
                                .get(&request.effect_key)
                                .context("nested trace parent missing")?;
                            self.trace_started(child, &effect).await?;
                            let execution =
                                if matches!(&effect.activity, ActivityReference::Workflow { .. }) {
                                    let child = parent
                                        .nested_workflows
                                        .get_mut(&request.effect_key)
                                        .context("nested continuation disappeared")?;
                                    self.execute_nested_activity(
                                        child,
                                        effect.clone(),
                                        logical_time,
                                        depth + 1,
                                    )
                                    .await?
                                } else {
                                    match self.execute_activity(&child_uri, &effect).await? {
                                        Execution::Observed(observation) => {
                                            NestedExecution::Observed(Box::new(observation))
                                        }
                                        Execution::Terminal { code, message } => {
                                            NestedExecution::Terminal { code, message }
                                        }
                                    }
                                };
                            let observation = match execution {
                                NestedExecution::Observed(observation) => *observation,
                                NestedExecution::Suspended(waits) => {
                                    return Ok(NestedExecution::Suspended(prefix_nested_waits(
                                        &request.effect_key,
                                        &waits,
                                    )));
                                }
                                NestedExecution::Terminal { code, message } => {
                                    self.trace_failed(&child_uri, &effect, &message).await?;
                                    return Ok(NestedExecution::Terminal { code, message });
                                }
                            };
                            self.trace_finished(&child_uri, &effect, &observation)
                                .await?;
                            logical_time = observation.observed_at;
                            let child = parent
                                .nested_workflows
                                .get_mut(&request.effect_key)
                                .context("nested continuation disappeared")?;
                            if let Err(error) =
                                child.apply_effect_observation(observation, logical_time)
                            {
                                return Ok(NestedExecution::Terminal {
                                    code: "invalidNestedActivityObservation",
                                    message: error.to_string(),
                                });
                            }
                        }
                    }
                    KernelOutcome::Guards(guards) => {
                        for guard in guards {
                            let observation = match self.execute_guard(&child_uri, &guard).await? {
                                Execution::Observed(observation) => observation,
                                Execution::Terminal { code, message } => {
                                    return Ok(NestedExecution::Terminal { code, message });
                                }
                            };
                            logical_time = observation.observed_at;
                            let child = parent
                                .nested_workflows
                                .get_mut(&request.effect_key)
                                .context("nested continuation disappeared")?;
                            if let Err(error) =
                                child.apply_guard_observation(&guard, observation.value)
                            {
                                return Ok(NestedExecution::Terminal {
                                    code: "invalidNestedGuardObservation",
                                    message: error.to_string(),
                                });
                            }
                        }
                    }
                    KernelOutcome::Suspended(waits) => {
                        return Ok(NestedExecution::Suspended(prefix_nested_waits(
                            &request.effect_key,
                            &waits,
                        )));
                    }
                    KernelOutcome::Completed(output) => {
                        break (output, "completed".to_owned());
                    }
                    KernelOutcome::Cancelled => {
                        break (json!({"disposition":"cancelled"}), "cancelled".to_owned());
                    }
                }
            };

            let mut observation = ActivityObservation {
                effect_key: request.effect_key.clone(),
                input_hash: request.input_hash.clone(),
                output_hash: hash_json(&output),
                output,
                disposition,
                provider: "embers-nested-workflow".to_owned(),
                implementation_hash: version.clone(),
                observed_at: Utc::now(),
                cost: None,
                deploy_id: format!("nested:{}", request.effect_key),
                signature: String::new(),
            };
            observation.signature = self.sign_json(&serde_json::to_value(&observation)?)?;
            self.continuation_store.put(
                ArtifactKind::Observation,
                &request.run_id,
                &request.effect_key,
                &observation,
            )?;
            parent.nested_workflows.remove(&request.effect_key);
            Ok(NestedExecution::Observed(Box::new(observation)))
        })
    }

    async fn execute_activity(
        &self,
        team_uri: &Uri,
        request: &EffectRequest,
    ) -> anyhow::Result<Execution<ActivityObservation>> {
        if let Some(pointer) = self.continuation_store.locate(
            ArtifactKind::Observation,
            &request.run_id,
            &request.effect_key,
        )? {
            return self
                .continuation_store
                .get(&pointer, &request.run_id, &request.effect_key)
                .map(Execution::Observed);
        }

        let input = match json_to_rho(&request.input) {
            Ok(input) => input,
            Err(error) => {
                return Ok(Execution::Terminal {
                    code: "nonPortableActivityInput",
                    message: error.to_string(),
                });
            }
        };
        let code = InvokeActivity {
            agents_team: team_uri.clone(),
            node_id: request.node_id.clone(),
            input,
        }
        .render()
        .map_err(anyhow::Error::from)?;
        let submit = self
            .submit_outboxed(
                &request.run_id,
                &request.effect_key,
                code,
                request
                    .timeout_millis
                    .map_or(self.deploy_finalization_timeout, Duration::from_millis),
            )
            .await?;
        let (deploy_id, forced_result) = match submit {
            SubmitOutcome::Finalized(deploy_id) => (deploy_id, None),
            SubmitOutcome::TimedOut(deploy_id) => (
                deploy_id,
                Some((
                    json!({"error":"activity exceeded its configured timeout"}),
                    "timedOut".to_owned(),
                )),
            ),
            SubmitOutcome::Errored(deploy_id) => (
                deploy_id.clone(),
                Some((
                    json!({
                        "error":"activity deploy errored on chain",
                        "deployId":deploy_id.to_string()
                    }),
                    "failed".to_owned(),
                )),
            ),
        };
        let observed_at = Utc::now();
        let (output, disposition) = if let Some(result) = forced_result {
            result
        } else {
            let raw: Value = self.read_deploy_result(deploy_id.clone()).await?;
            normalize_activity_result(raw)
        };
        let mut observation = ActivityObservation {
            effect_key: request.effect_key.clone(),
            input_hash: request.input_hash.clone(),
            output_hash: hash_json(&output),
            output,
            disposition,
            provider: "f1r3node-rholang".to_owned(),
            implementation_hash: hash_json(&serde_json::to_value(&request.activity)?),
            observed_at,
            cost: None,
            deploy_id: deploy_id.to_string(),
            signature: String::new(),
        };
        observation.signature = self.sign_json(&serde_json::to_value(&observation)?)?;
        self.continuation_store.put(
            ArtifactKind::Observation,
            &request.run_id,
            &request.effect_key,
            &observation,
        )?;
        Ok(Execution::Observed(observation))
    }

    async fn execute_guard(
        &self,
        team_uri: &Uri,
        request: &GuardRequest,
    ) -> anyhow::Result<Execution<GuardObservation>> {
        let object_id = format!("guard:{}", request.key);
        if let Some(pointer) = self.continuation_store.locate(
            ArtifactKind::Observation,
            &request.run_id,
            &object_id,
        )? {
            return self
                .continuation_store
                .get(&pointer, &request.run_id, &object_id)
                .map(Execution::Observed);
        }
        let value = match json_to_rho(&request.value) {
            Ok(value) => value,
            Err(error) => {
                return Ok(Execution::Terminal {
                    code: "nonPortableGuardValue",
                    message: error.to_string(),
                });
            }
        };
        let input = match json_to_rho(&request.input) {
            Ok(value) => value,
            Err(error) => {
                return Ok(Execution::Terminal {
                    code: "nonPortableGuardInput",
                    message: error.to_string(),
                });
            }
        };
        let state = match json_to_rho(&request.state) {
            Ok(value) => value,
            Err(error) => {
                return Ok(Execution::Terminal {
                    code: "nonPortableGuardState",
                    message: error.to_string(),
                });
            }
        };
        let code = InvokeGuard {
            agents_team: team_uri.clone(),
            transition_id: request.transition_id.clone(),
            value,
            input,
            state,
        }
        .render()?;
        let submit = self
            .submit_outboxed(
                &request.run_id,
                &object_id,
                code,
                self.deploy_finalization_timeout,
            )
            .await?;
        let deploy_id = match submit {
            SubmitOutcome::Finalized(deploy_id) => deploy_id,
            SubmitOutcome::TimedOut(deploy_id) => {
                return Ok(Execution::Terminal {
                    code: "guardTimedOut",
                    message: format!(
                        "guard deploy {deploy_id} was not finalized before its deadline"
                    ),
                });
            }
            SubmitOutcome::Errored(deploy_id) => {
                return Ok(Execution::Terminal {
                    code: "guardDeployErrored",
                    message: format!("guard deploy {deploy_id} errored on chain"),
                });
            }
        };
        let raw: Value = self.read_deploy_result(deploy_id.clone()).await?;
        let value = match raw {
            Value::Bool(value) => value,
            other => {
                return Ok(Execution::Terminal {
                    code: "invalidGuardResult",
                    message: format!("guard returned {}, not a Boolean", json_type_name(&other)),
                });
            }
        };
        let mut observation = GuardObservation {
            schema_version: "embers-agent-team-guard-observation-v1".to_owned(),
            key: request.key.clone(),
            value,
            deploy_id: deploy_id.to_string(),
            observed_at: Utc::now(),
            signature: String::new(),
        };
        observation.signature = self.sign_json(&serde_json::to_value(&observation)?)?;
        self.continuation_store.put(
            ArtifactKind::Observation,
            &request.run_id,
            &object_id,
            &observation,
        )?;
        Ok(Execution::Observed(observation))
    }

    async fn submit_outboxed(
        &self,
        run_id: &str,
        operation_id: &str,
        code: String,
        timeout: Duration,
    ) -> anyhow::Result<SubmitOutcome> {
        let outbox = if let Some(pointer) =
            self.continuation_store
                .locate(ArtifactKind::EffectOutbox, run_id, operation_id)?
        {
            self.continuation_store
                .get::<EffectOutbox>(&pointer, run_id, operation_id)?
        } else {
            let valid_after = self.write_client.clone().get_head_block_index().await?;
            let prepared = prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call();
            let outbox = EffectOutbox {
                schema_version: "embers-agent-team-effect-outbox-v1".to_owned(),
                operation_id: operation_id.to_owned(),
                signed_deploy: sign_prepared_contract(prepared.0, &self.service_key)?,
            };
            self.continuation_store.put(
                ArtifactKind::EffectOutbox,
                run_id,
                operation_id,
                &outbox,
            )?;
            outbox
        };
        ensure!(
            outbox.operation_id == operation_id,
            "effect outbox identity mismatch"
        );
        let (deploy_id, finalized) = self
            .submit_and_finalize_effect(outbox.signed_deploy, timeout)
            .await?;
        match finalized {
            Some(true) => Ok(SubmitOutcome::Errored(deploy_id)),
            Some(false) => Ok(SubmitOutcome::Finalized(deploy_id)),
            None => Ok(SubmitOutcome::TimedOut(deploy_id)),
        }
    }

    async fn fail_continuation(
        &self,
        continuation: &mut ContinuationEnvelope,
        code: &str,
        message: &str,
    ) -> anyhow::Result<Value> {
        let output = json!({
            "disposition": "failed",
            "error": {
                "code": code,
                "message": message,
            },
            "runId": continuation.run_id,
            "revision": continuation.revision,
        });
        let pointer = self.continuation_store.put(
            ArtifactKind::Output,
            &continuation.run_id,
            "result",
            &output,
        )?;
        self.checkpoint_and_advance(
            continuation,
            AdvanceMetadata {
                status: "failed",
                event_type: "runFailed",
                waits: &[],
                output: Some(&pointer),
            },
        )
        .await?;
        tracing::warn!(
            run_id = continuation.run_id,
            failure_code = code,
            failure_message = message,
            "durable Agent Team run reached a deterministic failure"
        );
        Ok(output)
    }

    pub(crate) async fn read_deploy_result<T: serde::de::DeserializeOwned + Send>(
        &self,
        deploy_id: DeployId,
    ) -> anyhow::Result<T> {
        let code = GetDeployResult {
            deploy_id: deploy_id.clone(),
        }
        .render()?;
        // Finalization events can precede the observer's readable state. Use
        // the configured finalization budget for visibility as well, rather
        // than failing an otherwise successful activity after a few seconds.
        let deadline = tokio::time::Instant::now() + self.deploy_finalization_timeout;
        loop {
            let result =
                tokio::time::timeout_at(deadline, self.read_client.get_data(code.clone())).await;
            match result {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(firefly_client::errors::ReadNodeError::ReturnValueMissing)) => {
                    if tokio::time::Instant::now() >= deadline {
                        anyhow::bail!(
                            "Finalized activity {deploy_id} is not yet readable on the observer; recovery will retry the same activity."
                        );
                    }
                    tokio::time::sleep_until(std::cmp::min(
                        deadline,
                        tokio::time::Instant::now() + Duration::from_millis(500),
                    ))
                    .await;
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => anyhow::bail!(
                    "Timed out waiting for finalized activity {deploy_id} to become readable on the observer; recovery will retry the same activity."
                ),
            }
        }
    }

    async fn checkpoint_and_advance(
        &self,
        continuation: &mut ContinuationEnvelope,
        metadata: AdvanceMetadata<'_>,
    ) -> anyhow::Result<()> {
        let expected_revision = continuation.revision;
        continuation.record_public_event(metadata.event_type);
        continuation.revision += 1;
        let object_id = format!("revision-{}", continuation.revision);
        let pointer = self.continuation_store.put(
            ArtifactKind::Continuation,
            &continuation.run_id,
            &object_id,
            continuation,
        )?;
        let mut wait_ids = metadata
            .waits
            .iter()
            .map(|wait| wait.id.clone())
            .collect::<Vec<_>>();
        wait_ids.sort();
        let due_at = metadata.waits.iter().filter_map(|wait| wait.due_at).min();
        let approval_authority_grants = metadata
            .waits
            .iter()
            .filter_map(|wait| wait.approval.as_ref().map(|approval| (wait, approval)))
            .flat_map(|(wait, approval)| {
                approval
                    .approvers
                    .iter()
                    .map(move |address| format!("{}|{}", wait.id, address))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        self.advance_run(
            &continuation.run_id,
            expected_revision,
            metadata.status,
            &continuation.definition_hash,
            &pointer,
            wait_ids,
            due_at,
            approval_authority_grants,
            metadata.output,
            metadata.event_type,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn advance_run(
        &self,
        run_id: &str,
        expected_revision: u64,
        status: &str,
        definition_hash: &str,
        continuation: &ArtifactPointer,
        wait_ids: Vec<String>,
        due_at: Option<DateTime<Utc>>,
        approval_authority_grants: Vec<String>,
        output: Option<&ArtifactPointer>,
        event_type: &str,
    ) -> anyhow::Result<()> {
        let next_revision = expected_revision
            .checked_add(1)
            .context("run revision overflow")?;
        let expected_revision_rho = i64::try_from(expected_revision)
            .context("run revision exceeds Rholang integer range")?;
        let output_content_hash = output.map(|pointer| pointer.content_hash.clone());
        let output_capability_hash = output.map(|pointer| pointer.capability_hash.clone());
        let expected = ExpectedRunAdvancement {
            revision: next_revision,
            status,
            definition_hash,
            continuation,
            wait_ids: &wait_ids,
            due_at: &due_at,
            approval_authority_grants: &approval_authority_grants,
            output_content_hash: &output_content_hash,
            output_capability_hash: &output_capability_hash,
            event_type,
        };
        let code = AdvanceRun {
            env_uri: self.uri.clone(),
            run_id: run_id.to_owned(),
            expected_revision: expected_revision_rho,
            status: status.to_owned(),
            definition_hash: definition_hash.to_owned(),
            continuation_content_hash: continuation.content_hash.clone(),
            continuation_capability_hash: continuation.capability_hash.clone(),
            wait_ids: wait_ids.clone(),
            due_at,
            approval_authority_grants: approval_authority_grants.clone(),
            output_content_hash: output_content_hash.clone(),
            output_capability_hash: output_capability_hash.clone(),
            event_type: event_type.to_owned(),
        }
        .render()?;
        let deploy_id = self
            .write_client
            .clone()
            .deploy(&self.service_key, DeployData::builder(code).build())
            .await?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {
                let state = self
                    .wait_for_run_state(run_id, "run advancement", |state| {
                        expected.verify(state).is_ok()
                    })
                    .await?;
                expected.verify(&state)?;
                Ok(())
            }
            Some(true) => bail!("run advancement deploy {deploy_id} errored on chain"),
            None => bail!("run advancement deploy {deploy_id} was not finalized"),
        }
    }

    fn sign_json(&self, value: &Value) -> anyhow::Result<String> {
        let canonical = canonical_json(value.clone());
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(&canonical)?).into();
        let signature = Secp256k1::new()
            .sign_ecdsa(Message::from_digest(digest), &self.service_key)
            .serialize_der();
        Ok(hex::encode(signature))
    }
}

fn normalize_activity_result(value: Value) -> (Value, String) {
    let Value::Object(mut object) = value else {
        return (value, "completed".to_owned());
    };
    let disposition = object
        .remove("disposition")
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "completed".to_owned());
    let output = object.remove("output").unwrap_or(Value::Object(object));
    (output, disposition)
}

fn prefix_nested_waits(execution_id: &str, waits: &[PendingWait]) -> Vec<PendingWait> {
    waits
        .iter()
        .cloned()
        .map(|mut wait| {
            wait.id = format!("nested:{execution_id}:{}", wait.id);
            wait
        })
        .collect()
}

const fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "Boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn json_to_rho(value: &Value) -> anyhow::Result<RhoValue> {
    Ok(match value {
        Value::Null => RhoValue::Nil,
        Value::Bool(value) => RhoValue::Bool(*value),
        Value::Number(value) => RhoValue::Int(
            value
                .as_i64()
                .context("Rholang activity input number is not an i64")?,
        ),
        Value::String(value) => RhoValue::String(value.clone()),
        Value::Array(values) => RhoValue::List(
            values
                .iter()
                .map(json_to_rho)
                .collect::<anyhow::Result<Vec<_>>>()?,
        ),
        Value::Object(values) => RhoValue::Map(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), json_to_rho(value)?)))
                .collect::<anyhow::Result<BTreeMap<_, _>>>()?,
        ),
    })
}

fn canonical_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect::<Map<_, _>>(),
        ),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_to_rholang_preserves_nested_portable_values() {
        let value = json!({"a":[1,true,null],"b":"text"});
        assert_eq!(
            json_to_rho(&value).expect("portable value"),
            RhoValue::Map(BTreeMap::from([
                (
                    "a".to_owned(),
                    RhoValue::List(vec![RhoValue::Int(1), RhoValue::Bool(true), RhoValue::Nil]),
                ),
                ("b".to_owned(), RhoValue::String("text".to_owned())),
            ]))
        );
    }

    #[test]
    fn structured_activity_result_extracts_disposition_and_output() {
        let (output, disposition) = normalize_activity_result(json!({
            "output": {"answer": 42},
            "disposition": "completed"
        }));
        assert_eq!(disposition, "completed");
        assert_eq!(output, json!({"answer":42}));
    }
}
