//! Grant-initiated runs: the channel wallet signs and pays the deploy, the
//! grantor owns the run, and the chain adjudicates the grant atomically with
//! run registration (`beginDurableRunUnderGrant` joins the runs and grants
//! states, so no revocation or cap race can interleave).
//!
//! Everything checked here before the deploy is fail-fast advice: a stale
//! grant cache on the channel side, or a revocation between prepare and send,
//! is caught by the chain clause, whose abort this module classifies honestly
//! after the fact by re-reading the grant. Only durable (Workflow v2) teams
//! are runnable under a grant: legacy GraphL runs never record an output
//! artifact, so their results could never be revealed for channel delivery.

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use firefly_client::models::Uri;
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use serde_json::Value;

use super::AgentsTeamsService;
use super::continuation_store::ArtifactKind;
use super::models::{
    DeploySignedRunUnderGrantReq,
    Grant,
    RunReq,
    RunResp,
    RunUnderGrantReq,
    RunUnderGrantResp,
};
use super::run_agents_team::DurableRunRequest;
use super::runtime::ContinuationEnvelope;
use super::workflow::WorkflowDefinition;
use crate::domain::common::{prepare_for_signing, record_trace, utc_now_protocol};

/// Milliseconds per UTC epoch day; the chain-side counter divides the deploy
/// timestamp (milliseconds) by this, and the advisory check below divides the
/// protocol timestamp (seconds) by 86 400 -- the same day number either way.
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/begin_durable_run_under_grant.rho")]
struct BeginDurableRunUnderGrant {
    env_uri: Uri,
    run_id: uuid::Uuid,
    grant_id: String,
    agents_team: Uri,
    created_at: DateTime<Utc>,
    definition_hash: String,
    request_content_hash: String,
    request_capability_hash: String,
    continuation_content_hash: String,
    continuation_capability_hash: String,
}

#[derive(Debug)]
pub enum RunUnderGrantError {
    /// No grant record exists under the id.
    GrantNotFound,
    /// The grant exists but is revoked.
    GrantRevoked,
    /// The grant's UTC-epoch-day run counter is at its cap.
    GrantCapsExceeded,
    /// The supplied channel context does not match the grant's channel or
    /// principal. Chain adjudication cannot see who typed a message, so this
    /// server-side check is the fail-fast half of the channel's routing duty.
    PrincipalMismatch,
    /// The grant names a team without a durable (Workflow v2) manifest; such
    /// runs have no revealable output and cannot serve a channel.
    TeamNotDurable,
    /// The deploy was accepted but finality was not observed in time. The run
    /// may still complete; the caller polls the run state to find out.
    NotFinalized(String),
    Internal(anyhow::Error),
}

impl std::fmt::Display for RunUnderGrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrantNotFound => write!(f, "grant not found"),
            Self::GrantRevoked => write!(f, "grant is revoked"),
            Self::GrantCapsExceeded => write!(f, "grant daily run cap exhausted"),
            Self::PrincipalMismatch => {
                write!(f, "channel context does not match the grant's principal")
            }
            Self::TeamNotDurable => {
                write!(f, "the granted team has no durable workflow deployment")
            }
            Self::NotFinalized(deploy_id) => {
                write!(f, "run deploy {deploy_id} was not finalized; poll the run state")
            }
            Self::Internal(err) => write!(f, "{err:?}"),
        }
    }
}

impl std::error::Error for RunUnderGrantError {}

impl From<anyhow::Error> for RunUnderGrantError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

fn validate_grant_for_run(
    grant: &Grant,
    request: &RunUnderGrantReq,
    now: DateTime<Utc>,
) -> Result<(), RunUnderGrantError> {
    if grant.revoked {
        return Err(RunUnderGrantError::GrantRevoked);
    }
    if request.channel_context.channel != grant.channel
        || request.channel_context.principal != grant.channel_principal
    {
        return Err(RunUnderGrantError::PrincipalMismatch);
    }
    if let Some(cap) = grant.caps.max_runs_per_day {
        let today = now.timestamp() / SECONDS_PER_DAY;
        if grant.runs_today.day == today && grant.runs_today.count >= cap {
            return Err(RunUnderGrantError::GrantCapsExceeded);
        }
    }
    Ok(())
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_under_grant(
        &self,
        request: RunUnderGrantReq,
    ) -> Result<RunUnderGrantResp, RunUnderGrantError> {
        record_trace!(request);

        let grant = self
            .get_grant(request.grant_id.clone())
            .await?
            .ok_or(RunUnderGrantError::GrantNotFound)?;
        let issued_at = utc_now_protocol();
        validate_grant_for_run(&grant, &request, issued_at)?;

        let manifest = self
            .get_team_manifest(grant.agents_team.clone())
            .await?
            .ok_or(RunUnderGrantError::TeamNotDurable)?;
        if manifest.protocol != "embers-agent-team-control-v1" {
            return Err(RunUnderGrantError::Internal(anyhow!(
                "unsupported Agent Team control protocol {}",
                manifest.protocol
            )));
        }
        let definition = WorkflowDefinition::from_json(manifest.definition)
            .map_err(anyhow::Error::from)?;
        definition
            .parsed()
            .validate_publishable()
            .map_err(anyhow::Error::from)?;
        if definition.definition_hash() != manifest.definition_hash {
            return Err(RunUnderGrantError::Internal(anyhow!(
                "Agent Team manifest definition hash mismatch"
            )));
        }

        let run_id = uuid::Uuid::now_v7();
        let run_id_text = run_id.to_string();
        let durable_request = DurableRunRequest {
            schema_version: "embers-agent-team-run-request-v3".to_owned(),
            run_id: run_id_text.clone(),
            agents_team: grant.agents_team.as_ref().to_owned(),
            prompt: request.prompt.clone(),
            created_at: issued_at,
            channel_context: Some(request.channel_context.clone()),
        };
        let request_pointer = self.continuation_store.put(
            ArtifactKind::RunRequest,
            &run_id_text,
            "request",
            &durable_request,
        )?;

        let continuation = ContinuationEnvelope::start(
            definition.parsed(),
            run_id_text.clone(),
            grant.agents_team.as_ref().to_owned(),
            manifest.definition_hash.clone(),
            Value::String(request.prompt.clone()),
        )
        .map_err(anyhow::Error::from)?;
        let continuation_pointer = self.continuation_store.put(
            ArtifactKind::Continuation,
            &run_id_text,
            "revision-0",
            &continuation,
        )?;

        let contract = BeginDurableRunUnderGrant {
            env_uri: self.uri.clone(),
            run_id,
            grant_id: request.grant_id.clone(),
            agents_team: grant.agents_team.clone(),
            created_at: issued_at,
            definition_hash: manifest.definition_hash.clone(),
            request_content_hash: request_pointer.content_hash.clone(),
            request_capability_hash: request_pointer.capability_hash.clone(),
            continuation_content_hash: continuation_pointer.content_hash.clone(),
            continuation_capability_hash: continuation_pointer.capability_hash.clone(),
        }
        .render()
        .map_err(anyhow::Error::from)?;

        let valid_after = self
            .write_client
            .clone()
            .get_head_block_index()
            .await
            .map_err(anyhow::Error::from)?;
        Ok(RunUnderGrantResp {
            run: RunResp {
                run_id: run_id_text,
                contract: prepare_for_signing()
                    .code(contract)
                    .valid_after_block_number(valid_after)
                    .call(),
                created_at: issued_at,
                durable: true,
                definition_hash: Some(manifest.definition_hash),
                request_content_hash: request_pointer.content_hash,
                request_capability_hash: request_pointer.capability_hash,
                continuation_content_hash: Some(continuation_pointer.content_hash),
                continuation_capability_hash: Some(continuation_pointer.capability_hash),
            },
            grant_id: request.grant_id,
            agents_team: grant.agents_team,
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_run_under_grant(
        &self,
        request: DeploySignedRunUnderGrantReq,
    ) -> Result<serde_json::Value, RunUnderGrantError> {
        record_trace!(request);

        if request.prepare_response.run.run_id.len() > 128 {
            return Err(RunUnderGrantError::Internal(anyhow!(
                "run id exceeds 128 bytes"
            )));
        }
        if request.prepare_response.grant_id != request.prepare_request.grant_id {
            return Err(RunUnderGrantError::Internal(anyhow!(
                "prepared grant id differs from the requested grant"
            )));
        }
        if !request.prepare_response.run.durable
            || request.prepare_response.run.definition_hash.is_none()
        {
            return Err(RunUnderGrantError::Internal(anyhow!(
                "grant runs are durable by construction"
            )));
        }

        let deploy_id = self
            .write_client
            .clone()
            .deploy_signed_contract(request.contract)
            .await
            .map_err(anyhow::Error::from)?;

        match self
            .wait_for_deploy_finalization(&deploy_id, self.deploy_finalization_timeout)
            .await
        {
            Some(false) => {}
            Some(true) => {
                // The clause aborted. Re-read the grant to name the cause
                // honestly: revocation and cap exhaustion are knowable from
                // the public record; whatever remains is internal.
                let grant = self.get_grant(request.prepare_request.grant_id.clone()).await?;
                return Err(match grant {
                    None => RunUnderGrantError::GrantNotFound,
                    Some(grant) if grant.revoked => RunUnderGrantError::GrantRevoked,
                    Some(grant)
                        if grant.caps.max_runs_per_day.is_some_and(|cap| {
                            grant.runs_today.day
                                == utc_now_protocol().timestamp() / SECONDS_PER_DAY
                                && grant.runs_today.count >= cap
                        }) =>
                    {
                        RunUnderGrantError::GrantCapsExceeded
                    }
                    Some(_) => RunUnderGrantError::Internal(anyhow!(
                        "run deploy {deploy_id} errored on chain"
                    )),
                });
            }
            None => {
                return Err(RunUnderGrantError::NotFinalized(deploy_id.to_string()));
            }
        }

        let run_id = request.prepare_response.run.run_id.clone();
        let lock = self
            .run_locks
            .entry(run_id.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let registered = self
            .wait_for_run_state(&run_id, "signed grant-run registration", |state| {
                verify_grant_run_registration(
                    state,
                    &request.prepare_response.agents_team,
                    &request.prepare_request.grant_id,
                    &request.prepare_response.run,
                )
                .is_ok()
            })
            .await?;
        verify_grant_run_registration(
            &registered,
            &request.prepare_response.agents_team,
            &request.prepare_request.grant_id,
            &request.prepare_response.run,
        )?;

        let resume_request = RunReq {
            prompt: request.prepare_request.prompt.clone(),
            agents_team: request.prepare_response.agents_team.clone(),
        };
        self.resume_durable_run(resume_request, request.prepare_response.run)
            .await
            .map_err(Into::into)
    }
}

/// The grant-run registration postcondition. Owner correctness cannot be
/// verified here -- the ledger scrubs `owner` from every read -- but the
/// clause registers `owner = grantor` by construction; what this checks is
/// that the observable record is exactly the prepared run, initiated under
/// exactly the requested grant.
fn verify_grant_run_registration(
    state: &crate::domain::agents_teams::models::RunState,
    agents_team: &Uri,
    grant_id: &str,
    prepared: &RunResp,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.run_id == prepared.run_id,
        "on-chain run id differs from the signed request"
    );
    anyhow::ensure!(
        state.grant_id.as_deref() == Some(grant_id),
        "on-chain run does not record the initiating grant"
    );
    anyhow::ensure!(
        &state.agents_team == agents_team,
        "on-chain Agent Team differs from the granted team"
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
    use super::*;
    use crate::domain::agents_teams::models::{
        ChannelContext,
        GrantAuthority,
        GrantCaps,
        GrantRunsToday,
    };
    use crate::domain::test_helpers::test_uri;

    fn grant() -> Grant {
        Grant {
            grant_id: "grant-1".to_owned(),
            grantor: "1111Grantor".to_owned(),
            grantee: "1111Grantee".to_owned(),
            channel: "chat".to_owned(),
            channel_principal: "a3f9".to_owned(),
            agents_team: test_uri(),
            caps: GrantCaps {
                max_runs_per_day: Some(2),
            },
            authority: GrantAuthority {
                reveal: true,
                commands: vec!["approve".to_owned()],
            },
            require_token_hash: None,
            min_auth: "none".to_owned(),
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            revoked: false,
            runs_today: GrantRunsToday { day: 0, count: 0 },
        }
    }

    fn request() -> RunUnderGrantReq {
        RunUnderGrantReq {
            grant_id: "grant-1".to_owned(),
            prompt: "hello".to_owned(),
            channel_context: ChannelContext {
                channel: "chat".to_owned(),
                principal: "a3f9".to_owned(),
                message_fingerprint: "fp-1".to_owned(),
                subject: None,
            },
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp_secs(1_754_900_000).expect("valid timestamp")
    }

    #[test]
    fn revoked_grant_fails_fast() {
        let mut grant = grant();
        grant.revoked = true;
        let error = validate_grant_for_run(&grant, &request(), now())
            .expect_err("revoked grant must fail");
        assert!(matches!(error, RunUnderGrantError::GrantRevoked));
    }

    #[test]
    fn mismatched_principal_fails_fast() {
        let mut request = request();
        request.channel_context.principal = "someone-else".to_owned();
        let error = validate_grant_for_run(&grant(), &request, now())
            .expect_err("principal mismatch must fail");
        assert!(matches!(error, RunUnderGrantError::PrincipalMismatch));
    }

    #[test]
    fn mismatched_channel_fails_fast() {
        let mut request = request();
        request.channel_context.channel = "mail".to_owned();
        let error = validate_grant_for_run(&grant(), &request, now())
            .expect_err("channel mismatch must fail");
        assert!(matches!(error, RunUnderGrantError::PrincipalMismatch));
    }

    #[test]
    fn exhausted_cap_today_fails_fast() {
        let mut grant = grant();
        grant.runs_today = GrantRunsToday {
            day: now().timestamp() / SECONDS_PER_DAY,
            count: 2,
        };
        let error =
            validate_grant_for_run(&grant, &request(), now()).expect_err("cap must be enforced");
        assert!(matches!(error, RunUnderGrantError::GrantCapsExceeded));
    }

    #[test]
    fn stale_day_counter_admits_a_new_day() {
        let mut grant = grant();
        grant.runs_today = GrantRunsToday {
            day: now().timestamp() / SECONDS_PER_DAY - 1,
            count: 2,
        };
        validate_grant_for_run(&grant, &request(), now())
            .expect("yesterday's exhausted counter must not block today");
    }

    #[test]
    fn uncapped_grant_never_exhausts() {
        let mut grant = grant();
        grant.caps.max_runs_per_day = None;
        grant.runs_today = GrantRunsToday {
            day: now().timestamp() / SECONDS_PER_DAY,
            count: 10_000,
        };
        validate_grant_for_run(&grant, &request(), now()).expect("uncapped grant must admit");
    }

    #[test]
    fn run_under_grant_template_renders_the_atomic_clause_call() {
        let code = BeginDurableRunUnderGrant {
            env_uri: test_uri(),
            run_id: uuid::Uuid::nil(),
            grant_id: "grant-1".to_owned(),
            agents_team: test_uri(),
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            definition_hash: "definition-hash".to_owned(),
            request_content_hash: "request-content".to_owned(),
            request_capability_hash: "request-capability".to_owned(),
            continuation_content_hash: "continuation-content".to_owned(),
            continuation_capability_hash: "continuation-capability".to_owned(),
        }
        .render()
        .expect("run under grant template should render");

        assert!(code.contains("beginDurableRunUnderGrant"));
        assert!(code.contains("\"grant-1\""));
        assert!(code.contains("\"definition-hash\""));
    }

    #[test]
    fn registration_verifier_requires_the_grant_link() {
        let prepared = RunResp {
            run_id: "run-1".to_owned(),
            contract: crate::domain::common::PreparedContract(Vec::new()),
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            durable: true,
            definition_hash: Some("definition-hash".to_owned()),
            request_content_hash: "request-content".to_owned(),
            request_capability_hash: "request-capability".to_owned(),
            continuation_content_hash: Some("continuation-content".to_owned()),
            continuation_capability_hash: Some("continuation-capability".to_owned()),
        };
        let state = crate::domain::agents_teams::models::RunState {
            run_id: "run-1".to_owned(),
            agents_team: test_uri(),
            grant_id: None,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            status: "running".to_owned(),
            events: Vec::new(),
            definition_hash: Some("definition-hash".to_owned()),
            revision: 0,
            request_content_hash: Some("request-content".to_owned()),
            request_capability_hash: Some("request-capability".to_owned()),
            continuation_content_hash: Some("continuation-content".to_owned()),
            continuation_capability_hash: Some("continuation-capability".to_owned()),
            command_id: None,
            command_content_hash: None,
            command_capability_hash: None,
            command_actor: None,
            command_class: None,
            command_wait_id: None,
            approval_authority_grants: Vec::new(),
            wait_ids: Vec::new(),
            due_at: None,
            output_content_hash: None,
            output_capability_hash: None,
        };

        let error = verify_grant_run_registration(&state, &test_uri(), "grant-1", &prepared)
            .expect_err("a run without the grant link must not verify");
        assert!(error.to_string().contains("initiating grant"));

        let mut linked = state;
        linked.grant_id = Some("grant-1".to_owned());
        verify_grant_run_registration(&linked, &test_uri(), "grant-1", &prepared)
            .expect("a correctly linked registration must verify");
    }
}
