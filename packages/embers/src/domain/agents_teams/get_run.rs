use std::time::Duration;

use firefly_client::models::Uri;
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::continuation_store::ArtifactKind;
use crate::domain::agents_teams::models::{RunEvent, RunState};
use crate::domain::agents_teams::runtime::ContinuationEnvelope;

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run.rho")]
struct GetRun {
    env_uri: Uri,
    run_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/list_runs.rho")]
struct ListRuns {
    env_uri: Uri,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub async fn get_run(
        &self,
        run_id: String,
    ) -> anyhow::Result<Option<crate::domain::agents_teams::models::RunState>> {
        let code = GetRun {
            env_uri: self.uri.clone(),
            run_id,
        }
        .render()?;
        // A finalized block may reach the observer before the read node has
        // incorporated its tuplespace state. Treat that bounded visibility lag
        // as retryable; a missing value after the window is a real absence.
        let state: Option<models::RunState> = self
            .read_client
            .get_data_or_none_with_retry(code, 5, Duration::from_millis(500))
            .await?
            .flatten();
        match state {
            Some(state) => Ok(Some(self.hydrate_run_events(state.into())?)),
            None => Ok(None),
        }
    }

    /// Wait until a finalized run write is observable with the expected
    /// postcondition. Read nodes can expose the previous finalized revision for
    /// a short interval, so retrying only absent values is insufficient.
    pub(crate) async fn wait_for_run_state<F>(
        &self,
        run_id: &str,
        description: &str,
        predicate: F,
    ) -> anyhow::Result<crate::domain::agents_teams::models::RunState>
    where
        F: Fn(&crate::domain::agents_teams::models::RunState) -> bool,
    {
        let mut last_state = None;
        for attempt in 0..=12 {
            if let Some(state) = self.get_run(run_id.to_owned()).await? {
                if predicate(&state) {
                    return Ok(state);
                }
                last_state = Some(state);
            }
            if attempt < 12 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        anyhow::bail!(
            "{description} for Agent Team run {run_id} was not observable after finality; last state: {last_state:?}"
        )
    }

    pub(crate) async fn list_runs(
        &self,
    ) -> anyhow::Result<Vec<crate::domain::agents_teams::models::RunState>> {
        let code = ListRuns {
            env_uri: self.uri.clone(),
        }
        .render()?;
        let states: Vec<models::RunState> = self.read_client.get_data(code).await?;
        let mut hydrated = Vec::with_capacity(states.len());
        for state in states {
            hydrated.push(self.hydrate_run_events(state.into())?);
        }
        Ok(hydrated)
    }

    fn hydrate_run_events(&self, mut state: RunState) -> anyhow::Result<RunState> {
        let Some(definition_hash) = state.definition_hash.as_deref() else {
            return Ok(state);
        };
        let (Some(content_hash), Some(capability_hash)) = (
            state.continuation_content_hash.as_deref(),
            state.continuation_capability_hash.as_deref(),
        ) else {
            return Ok(state);
        };

        let object_id = format!("revision-{}", state.revision);
        let pointer = self.locate_checked_artifact(
            ArtifactKind::Continuation,
            &state.run_id,
            &object_id,
            content_hash,
            capability_hash,
        )?;
        let continuation: ContinuationEnvelope =
            self.continuation_store
                .get(&pointer, &state.run_id, &object_id)?;
        continuation.validate_identity(
            &state.run_id,
            state.agents_team.as_ref(),
            definition_hash,
        )?;
        state.events = canonical_run_events(
            &continuation,
            &std::mem::take(&mut state.events),
            &state.status,
        );
        Ok(state)
    }
}

fn canonical_run_events(
    continuation: &ContinuationEnvelope,
    on_chain_events: &[RunEvent],
    status: &str,
) -> Vec<RunEvent> {
    let mut event_types = continuation
        .events
        .iter()
        .map(|event| event.event_type.clone())
        .collect::<Vec<_>>();
    if event_types
        .first()
        .is_none_or(|event| event != "runCreated")
    {
        event_types.insert(0, "runCreated".to_owned());
    }

    // These events can occur between durable checkpoints. Preserve any excess
    // occurrences from the on-chain coordination log while avoiding duplicates
    // already recorded by current continuations.
    for supplemental in ["commandClaimed", "commandRejected", "runReturned"] {
        let durable_count = event_types
            .iter()
            .filter(|event| event.as_str() == supplemental)
            .count();
        let on_chain_count = on_chain_events
            .iter()
            .filter(|event| event.event_type == supplemental)
            .count();
        event_types.extend(std::iter::repeat_n(
            supplemental.to_owned(),
            on_chain_count - durable_count.min(on_chain_count),
        ));
    }

    let terminal_event = match status {
        "completed" => Some("runCompleted"),
        "cancelled" => Some("runCancelled"),
        "failed" => Some("runFailed"),
        "returned" => Some("runReturned"),
        "suspended" => Some("runSuspended"),
        _ => None,
    };
    if let Some(terminal_event) = terminal_event
        && event_types
            .last()
            .is_none_or(|event| event != terminal_event)
    {
        event_types.push(terminal_event.to_owned());
    }

    event_types
        .into_iter()
        .enumerate()
        .map(|(sequence, event_type)| RunEvent {
            sequence: sequence as u64,
            event_type,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::agents_teams::workflow::WorkflowDefinition;

    fn continuation() -> ContinuationEnvelope {
        let workflow = WorkflowDefinition::from_json(
            json!({
                "schemaVersion": "2.0",
                "entryScopeId": "root",
                "interface": {"inputs": {"input": {"kind": "legacy"}}, "outputs": {"output": {"kind": "legacy"}}},
                "policies": {"defaultExecution": "durable", "visibility": "private"},
                "presentation": {"edges": {}, "nodes": {}},
                "routeGroups": {"done-route": {"id": "done-route", "mode": "allMatches", "noMatch": "fail"}},
                "scopes": {"root": {
                    "id": "root",
                    "label": "Root",
                    "kind": "root",
                    "initialNodeIds": ["input"],
                    "entryActivities": [],
                    "exitActivities": [],
                    "history": "none"
                }},
                "nodes": {
                    "input": {"id": "input", "label": "Input", "scopeId": "root", "kind": "input"},
                    "final": {"id": "final", "label": "Final", "scopeId": "root", "kind": "final"}
                },
                "transitions": {"done": {
                    "id": "done",
                    "sourceNodeId": "input",
                    "targetNodeId": "final",
                    "trigger": "completed",
                    "priority": 0,
                    "routeGroupId": "done-route"
                }},
                "types": {}
            })
            .to_string(),
        )
        .expect("workflow");
        ContinuationEnvelope::start(
            workflow.parsed(),
            "run-1".to_owned(),
            "rho:id:team".to_owned(),
            workflow.definition_hash(),
            json!(null),
        )
        .expect("continuation")
    }

    #[test]
    fn canonical_events_preserve_durable_audit_and_transient_coordination() {
        let mut continuation = continuation();
        continuation.record_public_event("approvalRecorded");
        let events = canonical_run_events(
            &continuation,
            &[
                RunEvent {
                    sequence: 0,
                    event_type: "runCreated".to_owned(),
                },
                RunEvent {
                    sequence: 1,
                    event_type: "commandClaimed".to_owned(),
                },
            ],
            "completed",
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            [
                "runCreated",
                "runKernelStarted",
                "approvalRecorded",
                "commandClaimed",
                "runCompleted"
            ]
        );
        assert!(
            events
                .iter()
                .enumerate()
                .all(|(sequence, event)| event.sequence == sequence as u64)
        );
    }
}
