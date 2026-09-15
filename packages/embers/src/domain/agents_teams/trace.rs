//! Owner-only live execution traces. Authorization uses an existing registry
//! operation, so deploying this backend does not replace the registry or lose
//! earlier runs. Private trace values never enter public run metadata.
use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use anyhow::ensure;
use chrono::Utc;
use dashmap::DashMap;
use firefly_client::models::{SignedCode, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::AgentsTeamsService;
use super::continuation_store::{ArtifactKind, F1r3driveContinuationStore};
use super::runtime::{ActivityObservation, ContinuationEnvelope, EffectPurpose, EffectRequest};
use super::workflow::WorkflowDefinition;
use crate::domain::common::{PreparedContract, prepare_for_signing};

static CLAIMS: LazyLock<DashMap<String, i64>> = LazyLock::new(DashMap::new);
static SESSIONS: LazyLock<DashMap<String, Session>> = LazyLock::new(DashMap::new);

struct Session {
    env: String,
    run_id: String,
    expires_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("Run not found.")]
    NotFound,
    #[error("Sign in with the wallet that owns this run.")]
    Unauthorized,
    #[error("Trace authorization expired or was already used. Unlock the history again.")]
    Expired,
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/authorize_trace.rho")]
struct AuthorizeTrace {
    env_uri: Uri,
    run_id: String,
    nonce: String,
}

pub struct TracePreparation {
    pub contract: PreparedContract,
    pub nonce: String,
    pub expires_at: i64,
}

/// A start and a finish are separate immutable records. A crash cannot replace
/// an earlier invocation; effect identity also fences cycles and retries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRecord {
    pub sequence: u64,
    pub phase: String,
    pub team_uri: String,
    pub request: EffectRequest,
    pub context: Value,
    pub observation: Option<ActivityObservation>,
}

impl F1r3driveContinuationStore {
    pub fn trace_records(&self, run_id: &str) -> anyhow::Result<Vec<TraceRecord>> {
        let _guard = self.trace_lock.lock().expect("trace journal lock poisoned");
        self.read_trace_records(run_id)
    }

    fn read_trace_records(&self, run_id: &str) -> anyhow::Result<Vec<TraceRecord>> {
        let mut records = Vec::new();
        for sequence in 0..100_000 {
            let id = format!("event-{sequence}");
            let Some(pointer) = self.locate(ArtifactKind::Trace, run_id, &id)? else {
                return Ok(records);
            };
            records.push(self.get(&pointer, run_id, &id)?);
        }
        anyhow::bail!("trace journal exceeds its record limit")
    }

    fn append_trace(&self, mut record: TraceRecord) -> anyhow::Result<()> {
        let _guard = self.trace_lock.lock().expect("trace journal lock poisoned");
        let existing = self.read_trace_records(&record.request.run_id)?;
        if let Some(previous) = existing.iter().find(|entry| {
            entry.request.effect_key == record.request.effect_key && entry.phase == record.phase
        }) {
            ensure!(
                previous.request.input_hash == record.request.input_hash,
                "trace replay input mismatch"
            );
            ensure!(
                previous.observation.as_ref().map(|o| &o.output_hash)
                    == record.observation.as_ref().map(|o| &o.output_hash),
                "trace replay output mismatch"
            );
            return Ok(());
        }
        record.sequence = existing.len() as u64;
        self.put(
            ArtifactKind::Trace,
            &record.request.run_id,
            &format!("event-{}", record.sequence),
            &record,
        )?;
        Ok(())
    }
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub async fn trace_started(
        &self,
        continuation: &ContinuationEnvelope,
        request: &EffectRequest,
    ) -> anyhow::Result<()> {
        let context = continuation
            .pending_effects
            .get(&request.effect_key)
            .map(|pending| match &pending.purpose {
                EffectPurpose::Node(token) | EffectPurpose::Lifecycle { token, .. } => {
                    json!({"iterations": token.iterations, "scopePath": token.scope_stack})
                }
                EffectPurpose::Compensation(_) => json!({"compensation": true}),
            })
            .unwrap_or(Value::Null);
        let store = self.continuation_store.clone();
        let record = TraceRecord {
            sequence: 0,
            phase: "started".into(),
            team_uri: continuation.team_uri.clone(),
            request: request.clone(),
            context,
            observation: None,
        };
        tokio::task::spawn_blocking(move || store.append_trace(record)).await?
    }

    pub async fn trace_finished(
        &self,
        team_uri: &Uri,
        request: &EffectRequest,
        observation: &ActivityObservation,
    ) -> anyhow::Result<()> {
        let store = self.continuation_store.clone();
        let record = TraceRecord {
            sequence: 0,
            phase: "finished".into(),
            team_uri: team_uri.as_ref().clone(),
            request: request.clone(),
            context: Value::Null,
            observation: Some(observation.clone()),
        };
        tokio::task::spawn_blocking(move || store.append_trace(record)).await?
    }

    pub async fn trace_failed(
        &self,
        team_uri: &Uri,
        request: &EffectRequest,
        message: &str,
    ) -> anyhow::Result<()> {
        let output = json!({"error": message});
        self.trace_finished(
            team_uri,
            request,
            &ActivityObservation {
                effect_key: request.effect_key.clone(),
                input_hash: request.input_hash.clone(),
                output_hash: super::runtime::hash_json(&output),
                output,
                disposition: "failed".into(),
                provider: "embers-runtime".into(),
                implementation_hash: String::new(),
                observed_at: Utc::now(),
                cost: None,
                deploy_id: String::new(),
                signature: String::new(),
            },
        )
        .await
    }

    pub async fn prepare_trace(&self, run_id: String) -> Result<TracePreparation, TraceError> {
        self.get_run(run_id.clone())
            .await?
            .ok_or(TraceError::NotFound)?;
        let nonce = random_token();
        let contract = AuthorizeTrace {
            env_uri: self.uri.clone(),
            run_id,
            nonce: nonce.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;
        Ok(TracePreparation {
            contract: prepare_for_signing()
                .code(contract)
                .valid_after_block_number(self.write_client.clone().get_head_block_index().await?)
                .call(),
            nonce,
            expires_at: Utc::now().timestamp() + 120,
        })
    }

    pub async fn authorize_trace(
        &self,
        run_id: String,
        prepared: TracePreparation,
        contract: SignedCode,
    ) -> Result<Value, TraceError> {
        let now = Utc::now().timestamp();
        if now >= prepared.expires_at {
            return Err(TraceError::Expired);
        }
        if contract.contract != prepared.contract.0 {
            return Err(
                anyhow::anyhow!("trace contract differs from the prepared contract").into(),
            );
        }
        CLAIMS.retain(|_, expiry| *expiry > now);
        if CLAIMS
            .insert(prepared.nonce.clone(), prepared.expires_at)
            .is_some()
        {
            return Err(TraceError::Expired);
        }
        self.get_run(run_id.clone())
            .await?
            .ok_or(TraceError::NotFound)?;
        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(contract)
            .await?;
        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => return Err(TraceError::Unauthorized),
            None => {
                return Err(anyhow::anyhow!(
                    "Trace authorization did not finalize. Try unlocking again."
                )
                .into());
            }
        }
        let ack: Value = self.read_deploy_result(deploy_id).await?;
        if ack["nonce"] != prepared.nonce || ack["runId"] != run_id {
            return Err(anyhow::anyhow!("trace acknowledgment mismatch").into());
        }
        let token = random_token();
        let expires_at = now + 600;
        SESSIONS.retain(|_, session| session.expires_at > now);
        SESSIONS.insert(
            token_hash(&token),
            Session {
                env: self.uri.as_ref().clone(),
                run_id,
                expires_at,
            },
        );
        Ok(json!({"accessToken": token, "expiresAt": expires_at * 1000}))
    }

    pub async fn read_trace(
        &self,
        run_id: String,
        authorization: &str,
        offset: usize,
    ) -> Result<Value, TraceError> {
        let token = authorization
            .strip_prefix("Bearer ")
            .ok_or(TraceError::Unauthorized)?;
        {
            let session = SESSIONS
                .get(&token_hash(token))
                .ok_or(TraceError::Expired)?;
            if session.expires_at <= Utc::now().timestamp() {
                return Err(TraceError::Expired);
            }
            if session.run_id != run_id || session.env != self.uri.as_ref().clone() {
                return Err(TraceError::Unauthorized);
            }
        }
        let run = self
            .get_run(run_id.clone())
            .await?
            .ok_or(TraceError::NotFound)?;
        let store = self.continuation_store.clone();
        let trace_run_id = run_id.clone();
        let records = tokio::task::spawn_blocking(move || store.trace_records(&trace_run_id))
            .await
            .map_err(anyhow::Error::from)??;
        let mut entries = Vec::<Value>::new();
        let mut positions = BTreeMap::new();
        for record in records {
            let key = record.request.effect_key.clone();
            let index = *positions.entry(key.clone()).or_insert_with(|| {
                let index = entries.len();
                entries.push(
                    json!({"id": key, "executionId": record.request.execution_id,
                    "nodeId": record.request.node_id, "agentName": record.request.node_label,
                    "teamUri": record.team_uri, "attempt": record.request.attempt,
                    "context": record.context, "sequence": record.sequence,
                    "input": record.request.input, "inputAvailable": true,
                    "outputAvailable": false, "status": "running"}),
                );
                index
            });
            if let Some(observation) = record.observation {
                entries[index]["output"] = observation.output;
                entries[index]["outputAvailable"] = json!(true);
                entries[index]["status"] = json!(observation.disposition);
                entries[index]["observedAt"] = json!(observation.observed_at);
            }
        }
        // Legacy runs have encrypted observations but no dedicated journal.
        // Preserve effect identity from the checkpoint; do not group by name.
        let mut legacy_missing = false;
        let mut legacy_entries = Vec::new();
        if let (Some(content), Some(capability)) = (
            &run.continuation_content_hash,
            &run.continuation_capability_hash,
        ) {
            let id = format!("revision-{}", run.revision);
            let pointer = self.locate_checked_artifact(
                ArtifactKind::Continuation,
                &run_id,
                &id,
                content,
                capability,
            )?;
            let continuation: ContinuationEnvelope =
                self.continuation_store.get(&pointer, &run_id, &id)?;
            let mut seen: HashSet<String> = positions.keys().cloned().collect();
            let manifest = self.get_team_manifest(run.agents_team.clone()).await?;
            let definition = manifest
                .map(|m| WorkflowDefinition::from_json(m.definition))
                .transpose()?
                .filter(|definition| definition.definition_hash() == continuation.definition_hash);
            for event in &continuation.events {
                if event.event_type != "effectRequested" {
                    continue;
                }
                let Some(key) = &event.detail_hash else {
                    continue;
                };
                if !seen.insert(key.clone()) {
                    continue;
                }
                let node = event
                    .node_id
                    .as_ref()
                    .and_then(|id| definition.as_ref()?.parsed().nodes.get(id));
                let observation = self
                    .continuation_store
                    .locate(ArtifactKind::Observation, &run_id, key)?
                    .map(|p| {
                        self.continuation_store
                            .get::<ActivityObservation>(&p, &run_id, key)
                    })
                    .transpose()?;
                let pending = continuation.pending_effects.get(key);
                legacy_entries.push(json!({"id": key, "executionId": "root", "nodeId": event.node_id,
                    "agentName": node.map(|n| n.label.as_str()).unwrap_or("Unknown agent"),
                    "teamUri": run.agents_team.as_ref(), "sequence": event.sequence, "legacy": true,
                    "inputAvailable": pending.is_some(), "input": pending.map(|p| &p.request.input),
                    "outputAvailable": observation.is_some(), "output": observation.as_ref().map(|o| &o.output),
                    "status": observation.as_ref().map(|o| o.disposition.as_str()).unwrap_or("unavailable")}));
                legacy_missing = true;
            }
        }
        // A resumed pre-journal run may contain both formats. Its older
        // observations precede the newly journaled invocations.
        legacy_entries.extend(entries);
        let mut entries = legacy_entries;
        for entry in &mut entries {
            if entry["status"] == "running"
                && matches!(run.status.as_str(), "completed" | "cancelled" | "failed")
            {
                entry["status"] = json!(if run.status == "completed" {
                    "unavailable"
                } else {
                    run.status.as_str()
                });
            }
        }
        let total = entries.len();
        let page: Vec<_> = entries.into_iter().skip(offset).take(100).collect();
        let next = offset + page.len();
        Ok(
            json!({"runId": run_id, "revision": run.revision, "status": run.status,
            "entries": page, "nextOffset": if next < total { Some(next) } else { None },
            "legacyNotice": if legacy_missing { Some("Older runs may not retain every input or completed nested-team detail.") } else { None }}),
        )
    }
}

fn random_token() -> String {
    hex::encode(secp256k1::SecretKey::new(&mut secp256k1::rand::rng()).secret_bytes())
}
fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::domain::test_helpers::*;

    fn service(
        nonce: &str,
        finalized: Option<bool>,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        let run = json!({"run_id":"run-1", "agents_team":test_uri().as_ref(), "created_at":0,
            "status":"running", "events":[], "revision":0});
        AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new().with_deploy_response(Ok(test_deploy_id())),
            read_client: MockReadNode::new()
                .on_code_containing("getRun", run)
                .on_code_containing("test-deploy-id", json!({"nonce":nonce, "runId":"run-1"})),
            observer_node_events: MockNodeEventSource::new()
                .with_wait_result("test-deploy-id", finalized),
            service_key: test_secret_key(),
            aes_encryption_key: [42; 32].into(),
            continuation_store: F1r3driveContinuationStore::open_for_test(
                tempfile::tempdir().unwrap().keep(),
                [7; 32],
                [9; 32],
            )
            .unwrap(),
            deploy_finalization_timeout: Duration::from_secs(1),
            run_locks: Arc::new(DashMap::new()),
            firesky_accounts: Arc::new(DashMap::new()),
        }
    }
    fn prepared(nonce: String) -> TracePreparation {
        TracePreparation {
            contract: PreparedContract(test_signed_code().contract),
            nonce,
            expires_at: Utc::now().timestamp() + 120,
        }
    }
    fn record(key: &str, execution: &str, attempt: u32) -> TraceRecord {
        let request: EffectRequest = serde_json::from_value(json!({
            "effectKey":key,"runId":"run-1","executionId":execution,"nodeId":"same-node","nodeLabel":"Critic",
            "activity":{"kind":"agent","agentId":"agent","agentVersion":"1","protocol":"simple-v1"},
            "input":"private input","inputHash":"input-hash","attempt":attempt,"authorities":[],
            "maxOutputBytes":null,"timeoutMillis":null
        })).unwrap();
        TraceRecord {
            sequence: 0,
            phase: "started".into(),
            team_uri: "team".into(),
            request,
            context: json!({"iterations":{"loop":attempt-1}}),
            observation: None,
        }
    }
    #[test]
    fn journal_preserves_repeated_and_nested_invocations_and_recovers_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            F1r3driveContinuationStore::open_for_test(dir.path(), [7; 32], [9; 32]).unwrap();
        for (key, parent, attempt) in [
            ("first", "root", 1),
            ("cycle", "root", 2),
            ("child", "first", 1),
        ] {
            store.append_trace(record(key, parent, attempt)).unwrap();
        }
        drop(store);
        let recovered =
            F1r3driveContinuationStore::open_for_test(dir.path(), [7; 32], [9; 32]).unwrap();
        recovered.append_trace(record("cycle", "root", 2)).unwrap();
        let records = recovered.trace_records("run-1").unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[2].request.execution_id, "first");
        assert_eq!(records[1].request.attempt, 2);
        assert!(recovered.trace_records("another-run").unwrap().is_empty());
    }
    #[tokio::test]
    async fn live_trace_auth_requires_owner_and_matching_ack() {
        let nonce = random_token();
        let denied = service(&nonce, Some(true));
        assert!(matches!(
            denied
                .authorize_trace("run-1".into(), prepared(nonce), test_signed_code())
                .await,
            Err(TraceError::Unauthorized)
        ));
        let nonce = random_token();
        let bad_ack = service("wrong-nonce", Some(false));
        assert!(
            bad_ack
                .authorize_trace("run-1".into(), prepared(nonce), test_signed_code())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn live_session_is_run_scoped_single_use_and_expires() {
        let nonce = random_token();
        let service = service(&nonce, Some(false));
        let session = service
            .authorize_trace("run-1".into(), prepared(nonce.clone()), test_signed_code())
            .await
            .unwrap();
        let token = session["accessToken"].as_str().unwrap();
        let auth = format!("Bearer {token}");
        assert!(service.read_trace("run-1".into(), &auth, 0).await.is_ok());
        assert!(matches!(
            service.read_trace("another-run".into(), &auth, 0).await,
            Err(TraceError::Unauthorized)
        ));
        assert!(matches!(
            service
                .authorize_trace("run-1".into(), prepared(nonce), test_signed_code())
                .await,
            Err(TraceError::Expired)
        ));
        SESSIONS.get_mut(&token_hash(token)).unwrap().expires_at = 0;
        assert!(matches!(
            service.read_trace("run-1".into(), &auth, 0).await,
            Err(TraceError::Expired)
        ));
    }
    #[test]
    fn authorization_contract_checks_existence_without_changing_execution() {
        let code = AuthorizeTrace {
            env_uri: test_uri(),
            run_id: "existing".into(),
            nonce: "nonce".into(),
        }
        .render()
        .unwrap();
        assert!(code.contains("\"getRun\""));
        assert!(code.contains("\"beginRun\""));
        assert!(!code.contains("\"finishRun\""));
        assert!(!code.contains("\"claimCommand\""));
    }
}
