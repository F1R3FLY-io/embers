//! Property tests over `workflow.rs`: canonicalization, JSON round-trips,
//! hash stability, the Tarjan hot-cycle rejection, and the retry-delay
//! computation observed through the kernel's actual retry scheduling.

use anyhow::{Context, ensure};
use chrono::{DateTime, Utc};
use proptest::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::generators::{
    arb_bounded_cycle_workflow,
    arb_branching_workflow,
    arb_hot_cycle_workflow,
    arb_linear_workflow,
    arb_scoped_workflow,
    document,
    root_only_scopes,
};
use super::kernel_model::observation;
use crate::domain::agents_teams::runtime::{
    ContinuationEnvelope,
    KernelOutcome,
    ResumeCommand,
    SuspensionKind,
};
use crate::domain::agents_teams::workflow::WorkflowDefinition;

fn arb_any_workflow() -> impl Strategy<Value = WorkflowDefinition> {
    prop_oneof![
        arb_linear_workflow().prop_map(|case| case.workflow),
        arb_branching_workflow().prop_map(|case| case.workflow),
        arb_bounded_cycle_workflow().prop_map(|case| case.workflow),
        arb_scoped_workflow().prop_map(|case| case.workflow),
    ]
}

/// Recursively reverses object key order. With `serde_json`'s
/// `preserve_order` feature active (pulled in by the API layer), this yields
/// a semantically identical document whose serialization differs — exactly
/// what `canonicalize` must erase.
fn reverse_key_order(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(reverse_key_order).collect())
        }
        Value::Object(values) => {
            let mut reversed = serde_json::Map::new();
            let mut entries: Vec<(String, Value)> = values.into_iter().collect();
            entries.reverse();
            for (key, entry) in entries {
                reversed.insert(key, reverse_key_order(entry));
            }
            Value::Object(reversed)
        }
        scalar => scalar,
    }
}

// -------------------------------------------------------
// Retry-delay determinism, observed through the kernel
// -------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct RetrySpec {
    max_attempts: u32,
    initial_delay_millis: u64,
    max_delay_millis: u64,
    backoff_multiplier_millis: u32,
    deterministic_jitter: bool,
}

fn arb_retry_spec() -> impl Strategy<Value = RetrySpec> {
    (
        2_u32..=4,
        0_u64..=3_000,
        0_u64..=3_000,
        1_000_u32..=3_000,
        any::<bool>(),
    )
        .prop_map(
            |(max_attempts, initial, extra, backoff_multiplier_millis, deterministic_jitter)| {
                RetrySpec {
                    max_attempts,
                    initial_delay_millis: initial,
                    max_delay_millis: initial + extra,
                    backoff_multiplier_millis,
                    deterministic_jitter,
                }
            },
        )
}

fn retry_workflow(spec: RetrySpec) -> WorkflowDefinition {
    let value = document(
        json!({
            "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
            "work": {
                "id": "work", "label": "Work", "scopeId": "root", "kind": "activity",
                "activity": {"kind": "builtin", "name": "compress"},
                "policy": {"retry": {
                    "maxAttempts": spec.max_attempts,
                    "initialDelayMillis": spec.initial_delay_millis,
                    "backoffMultiplierMillis": spec.backoff_multiplier_millis,
                    "maxDelayMillis": spec.max_delay_millis,
                    "deterministicJitter": spec.deterministic_jitter,
                    "retryOn": []
                }}
            },
            "out": {"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}
        }),
        json!({
            "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
            "done": {"id": "done", "mode": "allMatches", "noMatch": "fail"}
        }),
        json!({
            "start": {"id": "start", "sourceNodeId": "in", "targetNodeId": "work",
                      "trigger": "completed", "priority": 0, "routeGroupId": "start"},
            "done": {"id": "done", "sourceNodeId": "work", "targetNodeId": "out",
                     "trigger": "completed", "priority": 0, "routeGroupId": "done"}
        }),
        root_only_scopes("in"),
    );
    let workflow = WorkflowDefinition::new(value).expect("retry workflow parses");
    workflow
        .parsed()
        .validate_publishable()
        .expect("retry workflow is publishable");
    workflow
}

/// One scheduled retry as the kernel exposed it: the failed attempt's effect
/// key, the attempt number being scheduled, and the observed delay.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScheduledRetry {
    failed_effect_key: String,
    scheduled_attempt: u32,
    delay_millis: i64,
}

/// Fails every attempt of the single retrying activity and records each
/// retry deadline the kernel schedules. The run must end in the documented
/// exhaustion error (the `failed` disposition has no route), proving the
/// retry budget is finite.
fn drive_retry_schedule(workflow: &WorkflowDefinition) -> anyhow::Result<Vec<ScheduledRetry>> {
    let definition = workflow.parsed();
    let mut envelope = ContinuationEnvelope::start(
        definition,
        "run".into(),
        "rho:id:team".into(),
        workflow.definition_hash(),
        json!("request"),
    )?;
    let mut now = DateTime::<Utc>::UNIX_EPOCH;
    let mut schedule = Vec::new();
    let mut last_failed_key = String::new();

    for _ in 0..64 {
        let outcome = match envelope.run_until_blocked(definition, now) {
            Ok(outcome) => outcome,
            Err(error) => {
                ensure!(
                    error
                        .to_string()
                        .contains("produced disposition failed with no matching route group"),
                    "retry exhaustion must fail the run honestly: {error:#}"
                );
                return Ok(schedule);
            }
        };
        match outcome {
            KernelOutcome::Effects(requests) => {
                let request = requests.first().context("one retrying activity")?;
                last_failed_key.clone_from(&request.effect_key);
                envelope.apply_effect_observation(
                    observation(request, json!({"error": "boom"}), "failed", now),
                    now,
                )?;
            }
            KernelOutcome::Suspended(waits) => {
                let wait = waits.first().context("one retry wait")?;
                ensure!(
                    matches!(wait.kind, SuspensionKind::Retry),
                    "only retry suspensions are expected"
                );
                let due = wait.due_at.context("retries carry deadlines")?;
                let scheduled_attempt = wait.token.attempt;
                schedule.push(ScheduledRetry {
                    failed_effect_key: last_failed_key.clone(),
                    scheduled_attempt,
                    delay_millis: (due - now).num_milliseconds(),
                });
                now = due.max(now);
                envelope.apply_command(
                    ResumeCommand::TimerFired {
                        run_id: "run".into(),
                        wait_id: wait.id.clone(),
                        expected_deadline: due,
                    },
                    "scheduler",
                    now,
                )?;
            }
            other => anyhow::bail!("unexpected retry-drive outcome: {other:?}"),
        }
    }
    anyhow::bail!("retry drive did not exhaust within its bound")
}

/// Independent mirror of `runtime.rs`'s `retry_delay_millis`, asserted
/// against the deadlines the kernel actually schedules.
fn expected_delay_millis(spec: RetrySpec, effect_key: &str, attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(2);
    let multiplier = u128::from(spec.backoff_multiplier_millis).pow(exponent);
    let raw = u128::from(spec.initial_delay_millis).saturating_mul(multiplier)
        / 1_000_u128.pow(exponent);
    let capped = u64::try_from(raw.min(u128::from(spec.max_delay_millis)))
        .expect("capped delay fits u64");
    if !spec.deterministic_jitter || capped == 0 {
        return capped;
    }
    let digest = Sha256::digest(format!("{effect_key}:{attempt}").as_bytes());
    let sample = u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is exactly eight bytes"),
    );
    sample % (capped + 1)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// Canonicalization is idempotent and erases key order: re-parsing the
    /// canonical JSON is the identity, and a key-scrambled but semantically
    /// identical document canonicalizes to the same text.
    #[test]
    fn fv_inv_w1_canonicalize_idempotent(workflow in arb_any_workflow()) {
        let reparsed = WorkflowDefinition::from_json(workflow.json_ref())
            .expect("canonical JSON must re-parse");
        prop_assert_eq!(reparsed.json_ref(), workflow.json_ref());

        let scrambled_document = reverse_key_order(
            serde_json::from_str(workflow.json_ref()).expect("canonical JSON is JSON"),
        );
        let scrambled = WorkflowDefinition::new(scrambled_document)
            .expect("a key-scrambled document still parses");
        prop_assert_eq!(scrambled.json_ref(), workflow.json_ref());
    }

    /// `from_json ∘ json` is the identity on validated definitions.
    #[test]
    fn fv_inv_w2_json_round_trip(workflow in arb_any_workflow()) {
        let reparsed = WorkflowDefinition::from_json(workflow.json_ref())
            .expect("canonical JSON must re-parse");
        prop_assert_eq!(&reparsed, &workflow);
        prop_assert!(
            reparsed.parsed().validate_publishable().is_ok(),
            "round-tripping must preserve publishability"
        );
    }

    /// The definition hash is a pure function of the canonical document:
    /// stable across re-parsing and key scrambling.
    #[test]
    fn fv_inv_w3_definition_hash_stable(workflow in arb_any_workflow()) {
        let reparsed = WorkflowDefinition::from_json(workflow.json_ref())
            .expect("canonical JSON must re-parse");
        prop_assert_eq!(reparsed.definition_hash(), workflow.definition_hash());

        let scrambled_document = reverse_key_order(
            serde_json::from_str(workflow.json_ref()).expect("canonical JSON is JSON"),
        );
        let scrambled = WorkflowDefinition::new(scrambled_document)
            .expect("a key-scrambled document still parses");
        prop_assert_eq!(scrambled.definition_hash(), workflow.definition_hash());
    }

    /// The Tarjan progress-boundary check rejects a ring with no boundary
    /// and accepts the same ring once any one edge is bounded or any one
    /// node becomes a wait.
    #[test]
    fn fv_inv_w4_hot_cycle_rejected_boundary_accepted(case in arb_hot_cycle_workflow()) {
        let hot = WorkflowDefinition::new(case.hot.clone())
            .expect("a hot ring is still a valid draft");
        let rejection = hot.parsed().validate_publishable();
        let error = match rejection {
            Ok(()) => return Err(TestCaseError::fail(
                "a boundary-free cycle must be rejected".to_owned(),
            )),
            Err(error) => error,
        };
        prop_assert!(
            error.to_string().contains("can avoid"),
            "unexpected hot-cycle rejection: {:#}",
            error
        );

        let bounded = WorkflowDefinition::new(case.bounded_fix.clone())
            .expect("the bounded repair parses");
        prop_assert!(
            bounded.parsed().validate_publishable().is_ok(),
            "one bounded edge makes the ring publishable"
        );
        let waited = WorkflowDefinition::new(case.wait_fix)
            .expect("the wait repair parses");
        prop_assert!(
            waited.parsed().validate_publishable().is_ok(),
            "one wait node makes the ring publishable"
        );
    }

    /// The retry delay the kernel schedules is a deterministic function of
    /// (policy, effect key, attempt), equals the documented backoff formula,
    /// never exceeds `maxDelayMillis`, and — without jitter — never falls
    /// below the capped initial delay.
    #[test]
    fn fv_inv_w5_retry_delay_deterministic_and_bounded(spec in arb_retry_spec()) {
        let workflow = retry_workflow(spec);
        let first = drive_retry_schedule(&workflow);
        let second = drive_retry_schedule(&workflow);
        let (first, second) = match (first, second) {
            (Ok(first), Ok(second)) => (first, second),
            (first, second) => return Err(TestCaseError::fail(format!(
                "both retry drives must exhaust cleanly: {first:?} vs {second:?}"
            ))),
        };
        prop_assert_eq!(&first, &second, "retry schedules must be deterministic");
        prop_assert_eq!(
            first.len(),
            usize::try_from(spec.max_attempts - 1).expect("attempts fit usize"),
            "maxAttempts {} schedules exactly {} retries",
            spec.max_attempts,
            spec.max_attempts - 1
        );
        for retry in &first {
            let expected = expected_delay_millis(
                spec,
                &retry.failed_effect_key,
                retry.scheduled_attempt,
            );
            prop_assert_eq!(
                retry.delay_millis,
                i64::try_from(expected).expect("delays fit i64"),
                "attempt {} must follow the documented backoff formula",
                retry.scheduled_attempt
            );
            let delay = u64::try_from(retry.delay_millis).expect("delays are non-negative");
            prop_assert!(
                delay <= spec.max_delay_millis,
                "delay {} exceeds maxDelayMillis {}",
                delay,
                spec.max_delay_millis
            );
            if !spec.deterministic_jitter {
                let floor = spec.initial_delay_millis.min(spec.max_delay_millis);
                prop_assert!(
                    delay >= floor,
                    "un-jittered delay {} fell below the capped initial delay {}",
                    delay,
                    floor
                );
            }
        }
        if !spec.deterministic_jitter {
            for window in first.windows(2) {
                prop_assert!(
                    window[0].delay_millis <= window[1].delay_millis,
                    "un-jittered backoff must be non-decreasing: {:?}",
                    first
                );
            }
        }
    }
}
