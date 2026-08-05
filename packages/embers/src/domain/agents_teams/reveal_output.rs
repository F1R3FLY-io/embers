//! Owner-authenticated reveal of a run's final output value.
//!
//! The public run ledger is metadata-only by design: statuses, event types,
//! and content hashes. The output value itself rests as an encrypted
//! `ArtifactKind::Output` artifact whose hashes the terminal `advanceRun`
//! committed on the run record. This module lets exactly the run's on-chain
//! owner read that value back, through the platform's signed two-phase
//! protocol: prepare renders a `claimOutputReveal` contract, the client signs
//! it locally, and the send deploys it. Ownership is adjudicated by the
//! on-chain clause (the ledger scrubs `owner` from every read, so no off-chain
//! check is possible), which appends an actor-less `outputRevealed` event and
//! acknowledges over the deploy-id channel. The backend releases the decrypted
//! value only after reading that positive acknowledgment — against an
//! environment predating the clause no acknowledgment ever appears and the
//! reveal fails closed.
//!
//! No revision fence is needed: a run with a recorded output is terminal and
//! immutable. Send envelopes are single-use: a replayed envelope carries the
//! same signature and therefore the same deploy identity, so its deploy never
//! lands a second time — the backend detects that no new `outputRevealed`
//! event was produced (counted under the per-run lock, before and after the
//! deploy) and refuses without disclosure. Each prepare mints a fresh reveal
//! id, so revealing again is always one fresh prepare away.

use anyhow::{Context, anyhow};
use firefly_client::models::Uri;
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use serde::Deserialize;

use super::AgentsTeamsService;
use super::continuation_store::ArtifactKind;
use super::models::{DeploySignedRevealOutputReq, RevealOutputReq, RevealOutputResp, RunState};
use crate::domain::common::{prepare_for_signing, utc_now_protocol};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/claim_output_reveal.rho")]
struct ClaimOutputReveal {
    env_uri: Uri,
    run_id: String,
    reveal_id: String,
}

/// The `claimOutputReveal` clause's acknowledgment, read from the deploy-id
/// channel. Carrying the on-chain output hashes lets the backend verify, from
/// the same atomic read that proved authorization, that the record still
/// commits to exactly the artifact the prepare phase located.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevealAck {
    reveal_id: String,
    output_content_hash: String,
    output_capability_hash: String,
}

#[derive(Debug)]
pub enum RevealOutputError {
    /// No run record exists under the id.
    NotFound,
    /// The run exists but has no recorded output artifact (still running,
    /// suspended, resuming, or a legacy `returned` run).
    NotRevealable,
    /// The claim deploy was rejected on chain and the run record proves the
    /// only remaining abort cause: the signer is not the run's owner.
    Unauthorized,
    /// The send produced no new `outputRevealed` event: a replayed envelope
    /// (whose identical deploy never lands a second time) or an environment
    /// whose ledger predates the clause. Either way, no disclosure.
    Consumed,
    Internal(anyhow::Error),
}

impl std::fmt::Display for RevealOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "run capability not found"),
            Self::NotRevealable => write!(f, "run has no recorded output"),
            Self::Unauthorized => write!(f, "the signer does not hold reveal authority"),
            Self::Consumed => write!(
                f,
                "the reveal envelope was already consumed; prepare a fresh reveal"
            ),
            Self::Internal(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for RevealOutputError {}

impl From<anyhow::Error> for RevealOutputError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub async fn prepare_reveal_output(
        &self,
        request: RevealOutputReq,
    ) -> Result<RevealOutputResp, RevealOutputError> {
        let run = self
            .get_run(request.run_id.clone())
            .await?
            .ok_or(RevealOutputError::NotFound)?;
        let (content_hash, capability_hash) = revealable_hashes(&run)?;
        // Loud store-drift check before anything is signed: the on-chain
        // record commits to an artifact the store must actually hold.
        self.locate_checked_artifact(
            ArtifactKind::Output,
            &request.run_id,
            "result",
            &content_hash,
            &capability_hash,
        )?;

        let reveal_id = uuid::Uuid::now_v7().to_string();
        let issued_at = utc_now_protocol();
        let code = ClaimOutputReveal {
            env_uri: self.uri.clone(),
            run_id: request.run_id,
            reveal_id: reveal_id.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;
        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RevealOutputResp {
            reveal_id,
            contract: prepare_for_signing()
                .code(code)
                .valid_after_block_number(valid_after)
                .call(),
            issued_at,
            output_content_hash: content_hash,
            output_capability_hash: capability_hash,
        })
    }

    pub async fn deploy_signed_reveal_output(
        &self,
        request: DeploySignedRevealOutputReq,
    ) -> Result<serde_json::Value, RevealOutputError> {
        let run_id = request.prepare_request.run_id.clone();
        let lock = self
            .run_locks
            .entry(run_id.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        // Single-use fence, read under the run lock before deploying: a
        // replayed envelope carries the same signature and therefore the same
        // deploy identity, so its deploy never lands a second time and the
        // observer still reports the FIRST landing as finalized. The reveal
        // event count is the discriminator the chain gives us: a fresh claim
        // appends exactly one `outputRevealed`, a replay appends none.
        let before = self
            .get_run(run_id.clone())
            .await?
            .ok_or(RevealOutputError::NotFound)?;
        revealable_hashes(&before)?;
        let reveals_before = revealed_count(&before);

        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await?;
        match self
            .observer_node_events
            .wait_for_deploy(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // The clause aborted. Re-read the record to name the cause
                // honestly: absence and missing output are knowable without
                // authorization, so what remains is an unauthorized signer.
                let run = self.get_run(run_id.clone()).await?;
                return Err(match run {
                    None => RevealOutputError::NotFound,
                    Some(run) if revealable_hashes(&run).is_err() => {
                        RevealOutputError::NotRevealable
                    }
                    Some(_) => RevealOutputError::Unauthorized,
                });
            }
            None => {
                return Err(RevealOutputError::Internal(anyhow!(
                    "output reveal deploy {deploy_id} was not finalized"
                )));
            }
        }

        // A new reveal event, or no disclosure. The observer may lag the
        // finalized claim briefly, so the check retries; when no new event
        // ever appears the envelope was a replay (or the ledger predates the
        // clause) and the reveal fails closed.
        self.wait_for_run_state(&run_id, "output reveal event", |run| {
            revealed_count(run) > reveals_before
        })
        .await
        .map_err(|_| RevealOutputError::Consumed)?;

        // Positive on-chain acknowledgment as defense in depth: the value is
        // released only against the clause's own answer, verified below
        // against the prepare echo.
        let ack: RevealAck = self
            .read_deploy_result(deploy_id)
            .await
            .context("output reveal acknowledgment was not readable")?;
        if ack.reveal_id != request.prepare_response.reveal_id {
            return Err(RevealOutputError::Internal(anyhow!(
                "output reveal acknowledgment does not match the prepared reveal"
            )));
        }
        if ack.output_content_hash != request.prepare_response.output_content_hash
            || ack.output_capability_hash != request.prepare_response.output_capability_hash
        {
            return Err(RevealOutputError::Internal(anyhow!(
                "on-chain output hashes changed between prepare and claim"
            )));
        }

        let pointer = self.locate_checked_artifact(
            ArtifactKind::Output,
            &run_id,
            "result",
            &ack.output_content_hash,
            &ack.output_capability_hash,
        )?;
        let value: serde_json::Value = self.continuation_store.get(&pointer, &run_id, "result")?;
        Ok(value)
    }
}

fn revealable_hashes(run: &RunState) -> Result<(String, String), RevealOutputError> {
    match (
        run.output_content_hash.as_deref(),
        run.output_capability_hash.as_deref(),
    ) {
        (Some(content), Some(capability)) => Ok((content.to_owned(), capability.to_owned())),
        _ => Err(RevealOutputError::NotRevealable),
    }
}

fn revealed_count(run: &RunState) -> usize {
    run.events
        .iter()
        .filter(|event| event.event_type == "outputRevealed")
        .count()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use dashmap::DashMap;
    use serde_json::json;

    use super::*;
    use crate::domain::common::PreparedContract;
    use crate::domain::test_helpers::*;

    fn make_service(
        read_node: MockReadNode,
        event_source: MockNodeEventSource,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new().with_deploy_response(Ok(test_deploy_id())),
            read_client: read_node,
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

    fn reveal_request() -> DeploySignedRevealOutputReq {
        DeploySignedRevealOutputReq {
            contract: test_signed_code(),
            prepare_request: RevealOutputReq {
                run_id: "run-1".to_owned(),
            },
            prepare_response: RevealOutputResp {
                reveal_id: uuid::Uuid::nil().to_string(),
                contract: PreparedContract(Vec::new()),
                issued_at: DateTime::<Utc>::UNIX_EPOCH,
                output_content_hash: "output-content".to_owned(),
                output_capability_hash: "output-capability".to_owned(),
            },
        }
    }

    /// A completed run as the read node serves it. Shaped without a definition
    /// hash so `get_run` does not hydrate events through the continuation
    /// store, which holds no artifacts in these tests.
    fn completed_run_json() -> serde_json::Value {
        let uri = test_uri();
        let uri: &str = uri.as_ref();
        json!({
            "run_id": "run-1",
            "agents_team": uri,
            "created_at": 0,
            "status": "completed",
            "events": [{"sequence": 0, "type": "runCompleted"}],
            "revision": 3,
            "output_content_hash": "output-content",
            "output_capability_hash": "output-capability"
        })
    }

    #[test]
    fn claim_output_reveal_template_renders_the_clause_and_the_deploy_id_ack() {
        let code = ClaimOutputReveal {
            env_uri: test_uri(),
            run_id: "run-1".to_owned(),
            reveal_id: "reveal-1".to_owned(),
        }
        .render()
        .expect("reveal template should render");

        assert!(code.contains("claimOutputReveal"));
        assert!(code.contains("rho:rchain:deployId"));
        assert!(code.contains("forward!(ack, (*deployId).toString())"));
        let uri = test_uri();
        let uri_str: &str = uri.as_ref();
        assert!(code.contains(uri_str));
    }

    #[tokio::test]
    async fn errored_claim_deploy_against_a_present_output_is_unauthorized() {
        let read_node = MockReadNode::new().on_code_containing("getRun", completed_run_json());
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(true));
        let service = make_service(read_node, event_source);

        let error = service
            .deploy_signed_reveal_output(reveal_request())
            .await
            .expect_err("an on-chain abort must not disclose the output");

        assert!(
            matches!(error, RevealOutputError::Unauthorized),
            "unexpected classification: {error:?}"
        );
    }

    #[tokio::test]
    async fn unfinalized_claim_deploy_is_internal_not_disclosure() {
        let read_node = MockReadNode::new().on_code_containing("getRun", completed_run_json());
        let event_source = MockNodeEventSource::new().with_wait_result("test-deploy-id", None);
        let service = make_service(read_node, event_source);

        let error = service
            .deploy_signed_reveal_output(reveal_request())
            .await
            .expect_err("an unfinalized claim must not disclose the output");

        assert!(
            matches!(&error, RevealOutputError::Internal(err) if err.to_string().contains("not finalized")),
            "unexpected classification: {error:?}"
        );
    }

    #[tokio::test]
    async fn replayed_envelope_produces_no_new_event_and_is_refused() {
        // The deploy "finalizes" (the observer still reports the FIRST landing
        // of the identical deploy), but the reveal-event count never moves —
        // the replay shape, and equally the pre-clause-environment shape.
        // Either way: refusal, no disclosure.
        let read_node = MockReadNode::new().on_code_containing("getRun", completed_run_json());
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(false));
        let service = make_service(read_node, event_source);

        let error = service
            .deploy_signed_reveal_output(reveal_request())
            .await
            .expect_err("a replayed envelope must not disclose the output");

        assert!(
            matches!(&error, RevealOutputError::Consumed),
            "unexpected classification: {error:?}"
        );
    }

    #[tokio::test]
    async fn missing_acknowledgment_fails_closed() {
        // The claim visibly landed (the reveal-event count moved), but the
        // deploy-id channel yields no acknowledgment — store or read drift.
        // Defense in depth: the value is released only against the clause's
        // own answer, so this still refuses.
        let mut revealed = completed_run_json();
        revealed["events"] = serde_json::json!([
            {"sequence": 0, "type": "runCompleted"},
            {"sequence": 1, "type": "outputRevealed"}
        ]);
        let read_node = MockReadNode::new().on_code_containing_values_then(
            "getRun",
            vec![completed_run_json()],
            revealed,
        );
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(false));
        let service = make_service(read_node, event_source);

        let error = service
            .deploy_signed_reveal_output(reveal_request())
            .await
            .expect_err("a missing acknowledgment must not disclose the output");

        assert!(
            matches!(&error, RevealOutputError::Internal(err)
                if err.to_string().contains("acknowledgment was not readable")),
            "unexpected classification: {error:?}"
        );
    }
}
