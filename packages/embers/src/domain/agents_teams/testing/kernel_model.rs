//! Executable mirrors of the frontend TLA⁺ invariants, asserted against the
//! real kernel ([`ContinuationEnvelope`]) while a scripted driver plays the
//! roles of orchestrator, scheduler, approvers, and activity providers.
//!
//! Every checker here is a genuine assertion over kernel state — the token
//! conservation checker recounts the live population from the envelope and
//! compares it to the kernel's stored counters; the replay checker compares
//! serialized envelope bytes — so a checker can only pass by the kernel
//! actually maintaining the property.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use anyhow::{Context, bail, ensure};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::generators::{
    BranchingWorkflow,
    DriverScript,
    JoinKind,
    LateChoice,
    branch_label,
    branch_node_id,
};
use crate::domain::agents_teams::runtime::{
    ActivityObservation,
    ContinuationEnvelope,
    EffectPurpose,
    EffectRequest,
    KernelOutcome,
    LifecycleStage,
    PendingWait,
    ResumeCommand,
    SuspensionKind,
    Token,
    TokenPurpose,
    hash_json,
};
use crate::domain::agents_teams::workflow::WorkflowDefinition;

/// How the driver fabricates activity outputs.
#[derive(Debug, Clone)]
pub(crate) enum OutputPlan {
    /// Echo the effect request's input back as the output.
    Echo,
    /// A fixed output per node id; nodes without an entry echo their input.
    PerNode(BTreeMap<String, Value>),
    /// `{"counter": k}` with `k` incremented per observation — drives the
    /// counter-guarded durable-yield cycles.
    Counter,
}

#[derive(Debug, Clone)]
pub(crate) struct DrivePlan {
    /// Node-id preference order used to pick which pending effect to observe
    /// next; effects not named fall back to script-indexed selection.
    pub observation_order: Vec<String>,
    pub output_plan: OutputPlan,
    pub script: DriverScript,
    /// Serialize + deserialize the envelope at this iteration and continue
    /// from the round-tripped copy — the persisted-resume path.
    pub checkpoint_at: Option<usize>,
    pub max_iterations: usize,
}

impl DrivePlan {
    pub(crate) fn scripted(script: DriverScript) -> Self {
        Self {
            observation_order: Vec::new(),
            output_plan: OutputPlan::Echo,
            script,
            checkpoint_at: None,
            max_iterations: 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DriveEnd {
    Completed(Value),
    Cancelled,
}

#[derive(Debug)]
pub(crate) struct DriveResult {
    pub end: DriveEnd,
    pub envelope: ContinuationEnvelope,
    /// Node ids of the effects observed, in observation order.
    pub observed_nodes: Vec<String>,
    /// Number of (iteration × scope instance) conservation assertions made —
    /// a vacuity guard: scoped runs must report a positive count.
    pub conservation_samples: usize,
}

/// Mirrors the `observation` helper in the `runtime.rs` test module: a
/// well-formed [`ActivityObservation`] answering `request`.
pub(crate) fn observation(
    request: &EffectRequest,
    output: Value,
    disposition: &str,
    observed_at: DateTime<Utc>,
) -> ActivityObservation {
    ActivityObservation {
        effect_key: request.effect_key.clone(),
        input_hash: request.input_hash.clone(),
        output_hash: hash_json(&output),
        output,
        disposition: disposition.to_owned(),
        provider: "test-provider".into(),
        implementation_hash: "test-implementation".into(),
        observed_at,
        cost: None,
        deploy_id: "test-deploy".into(),
        signature: "test-signature".into(),
    }
}

fn next_choice(script: &DriverScript, cursor: &mut usize) -> u8 {
    let choice = script.choices.get(*cursor).copied().unwrap_or(0);
    *cursor += 1;
    choice
}

fn choose_effect<'a>(
    requests: &'a [EffectRequest],
    order: &[String],
    choice: u8,
) -> &'a EffectRequest {
    for preferred in order {
        if let Some(request) = requests
            .iter()
            .find(|request| &request.node_id == preferred)
        {
            return request;
        }
    }
    &requests[usize::from(choice) % requests.len()]
}

/// Drives one run of `workflow` to a terminal, interpreting `plan.script`
/// against the live kernel state so every issued command is valid by
/// construction. Token conservation (INV-K1) is asserted after every
/// quiescent step. The virtual clock starts at the UNIX epoch and advances
/// deterministically, so two drives of the same plan are bit-equal.
pub(crate) fn drive(
    workflow: &WorkflowDefinition,
    input: Value,
    plan: &DrivePlan,
) -> anyhow::Result<DriveResult> {
    let definition = workflow.parsed();
    let mut envelope = ContinuationEnvelope::start(
        definition,
        "run".into(),
        "rho:id:team".into(),
        workflow.definition_hash(),
        input,
    )?;
    let mut now = DateTime::<Utc>::UNIX_EPOCH;
    let mut cursor = 0_usize;
    let mut counter = 0_i64;
    let mut decision_sequence = 0_usize;
    let mut observed_nodes = Vec::new();
    let mut conservation_samples = 0_usize;

    for iteration in 0..plan.max_iterations {
        if plan.checkpoint_at == Some(iteration) {
            let bytes = serde_json::to_vec(&envelope)
                .context("mid-run envelope must serialize")?;
            envelope = serde_json::from_slice(&bytes)
                .context("mid-run envelope must deserialize")?;
        }
        if plan.script.cancel_at == Some(iteration) {
            envelope.apply_command(
                ResumeCommand::Cancel {
                    run_id: "run".into(),
                    reason: "scripted cancellation".into(),
                },
                "owner",
                now,
            )?;
        }

        let outcome = envelope.run_until_blocked(definition, now)?;
        conservation_samples += check_token_conservation(&envelope)?;
        now += chrono::Duration::seconds(1);

        match outcome {
            KernelOutcome::Completed(value) => {
                return Ok(DriveResult {
                    end: DriveEnd::Completed(value),
                    envelope,
                    observed_nodes,
                    conservation_samples,
                });
            }
            KernelOutcome::Cancelled => {
                return Ok(DriveResult {
                    end: DriveEnd::Cancelled,
                    envelope,
                    observed_nodes,
                    conservation_samples,
                });
            }
            KernelOutcome::Effects(requests) => {
                ensure!(!requests.is_empty(), "an Effects outcome carries requests");
                let choice = next_choice(&plan.script, &mut cursor);
                let request =
                    choose_effect(&requests, &plan.observation_order, choice).clone();
                let output = match &plan.output_plan {
                    OutputPlan::Echo => request.input.clone(),
                    OutputPlan::PerNode(outputs) => outputs
                        .get(&request.node_id)
                        .cloned()
                        .unwrap_or_else(|| request.input.clone()),
                    OutputPlan::Counter => {
                        counter += 1;
                        json!({"counter": counter})
                    }
                };
                observed_nodes.push(request.node_id.clone());
                envelope
                    .apply_effect_observation(observation(&request, output, "completed", now), now)?;
            }
            KernelOutcome::Guards(requests) => {
                ensure!(!requests.is_empty(), "a Guards outcome carries requests");
                for request in &requests {
                    let verdict = next_choice(&plan.script, &mut cursor).is_multiple_of(2);
                    envelope.apply_guard_observation(request, verdict)?;
                }
            }
            KernelOutcome::Suspended(mut waits) => {
                ensure!(!waits.is_empty(), "a Suspended outcome carries waits");
                waits.sort_by(|left, right| left.id.cmp(&right.id));
                let choice = next_choice(&plan.script, &mut cursor);
                let wait = waits[usize::from(choice) % waits.len()].clone();
                resume_one_wait(
                    &mut envelope,
                    &wait,
                    next_choice(&plan.script, &mut cursor),
                    &mut now,
                    &mut decision_sequence,
                )?;
            }
        }
    }
    bail!(
        "driver iteration bound {} exceeded without reaching a terminal — the run diverged",
        plan.max_iterations
    )
}

fn resume_one_wait(
    envelope: &mut ContinuationEnvelope,
    wait: &PendingWait,
    choice: u8,
    now: &mut DateTime<Utc>,
    decision_sequence: &mut usize,
) -> anyhow::Result<()> {
    match wait.kind {
        SuspensionKind::Signal => envelope.apply_command(
            ResumeCommand::Signal {
                run_id: "run".into(),
                wait_id: wait.id.clone(),
                name: wait
                    .signal
                    .clone()
                    .unwrap_or_else(|| "signal".into()),
                payload: Value::Null,
            },
            "caller",
            *now,
        ),
        SuspensionKind::Event => envelope.apply_command(
            ResumeCommand::Event {
                run_id: "run".into(),
                wait_id: wait.id.clone(),
                name: wait
                    .signal
                    .clone()
                    .unwrap_or_else(|| "event".into()),
                payload: Value::Null,
            },
            "caller",
            *now,
        ),
        SuspensionKind::Timer | SuspensionKind::Retry | SuspensionKind::DurableYield => {
            let due = wait
                .due_at
                .with_context(|| format!("scheduler wait {} has no deadline", wait.id))?;
            if due > *now {
                *now = due;
            }
            envelope.apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    expected_deadline: due,
                },
                "scheduler",
                *now,
            )
        }
        SuspensionKind::Approval => {
            let approval = wait
                .approval
                .as_ref()
                .context("approval wait carries approval state")?;
            if choice % 2 == 1 {
                return envelope.apply_command(
                    ResumeCommand::Reject {
                        run_id: "run".into(),
                        wait_id: wait.id.clone(),
                        payload: Value::Null,
                    },
                    approval
                        .approvers
                        .iter()
                        .next()
                        .map_or("owner", String::as_str),
                    *now,
                );
            }
            let actor = approval
                .approvers
                .iter()
                .find(|approver| !approval.decisions.contains_key(*approver))
                .map_or("owner", String::as_str)
                .to_owned();
            let decision_id = format!("decision-{decision_sequence}");
            *decision_sequence += 1;
            envelope.apply_command(
                ResumeCommand::Approve {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    decision_id,
                    payload: Value::Null,
                },
                &actor,
                *now,
            )
        }
    }
}

// -------------------------------------------------------
// INV-K1 — token conservation (CounterFaithful)
// -------------------------------------------------------

/// The live-episode population of one instance: its directly attributed
/// carrier tokens plus one for each live child instance (memoized over the
/// instance forest).
fn episode_count<'a>(
    instance: &'a str,
    direct: &BTreeMap<&str, usize>,
    children: &BTreeMap<&str, Vec<&'a str>>,
    memo: &mut BTreeMap<&'a str, usize>,
) -> usize {
    if let Some(&count) = memo.get(instance) {
        return count;
    }
    let mut total = direct.get(instance).copied().unwrap_or(0);
    if let Some(kids) = children.get(instance) {
        for kid in kids.clone() {
            if episode_count(kid, direct, children, memo) > 0 {
                total += 1;
            }
        }
    }
    memo.insert(instance, total);
    total
}

/// Asserts, for every live scope instance, that the kernel's stored
/// `active_tokens` counter (the TLA⁺ model's `alpha`) equals the live token
/// population attributable to that instance, recounted from scratch out of
/// the envelope. Returns the number of instances checked so callers can
/// reject vacuous passes.
///
/// The live population of an instance `I` is:
///
/// - every live carrier token directly attributed to `I` — a token in the
///   queue, a pending wait, a pending Node/Lifecycle effect, or an arrival
///   parked in an **open** join, whose attribution is its scope-stack top
///   (entry-lifecycle carriers run before the child is populated, so their
///   top is already the parent; exit-lifecycle carriers still carry the
///   finished child on top, so they attribute one level down); plus
/// - one for each live **child** scope instance of `I` that itself has a
///   non-zero population — a nested episode is one carrier of its parent
///   until it finishes.
///
/// Arrivals in a closed quorum/race join were consumed when the join fired
/// (the kernel decremented the counter then), so they are deliberately not
/// live. Cancellation clears every token store but keeps `scopes` for
/// compensation bookkeeping, so a cancelling envelope is out of scope.
pub(crate) fn check_token_conservation(
    envelope: &ContinuationEnvelope,
) -> anyhow::Result<usize> {
    if envelope.cancelling {
        return Ok(0);
    }

    let mut carriers: Vec<&Token> = Vec::with_capacity(
        envelope.queue.len() + envelope.pending_waits.len() + envelope.pending_effects.len(),
    );
    carriers.extend(envelope.queue.iter());
    carriers.extend(envelope.pending_waits.values().map(|wait| &wait.token));
    for pending in envelope.pending_effects.values() {
        match &pending.purpose {
            EffectPurpose::Node(token) | EffectPurpose::Lifecycle { token, .. } => {
                carriers.push(token);
            }
            EffectPurpose::Compensation(_) => {}
        }
    }
    for join in envelope.joins.values() {
        if !join.closed {
            carriers.extend(join.arrivals.values());
        }
    }

    // Parent attribution: consecutive scope-stack pairs, checked consistent.
    let mut parent: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    for token in &carriers {
        for (index, instance) in token.scope_stack.iter().enumerate() {
            ensure!(
                envelope.scopes.contains_key(instance),
                "token {} references vanished scope instance {instance}",
                token.id
            );
            let parent_id = index
                .checked_sub(1)
                .map(|previous| token.scope_stack[previous].as_str());
            match parent.entry(instance.as_str()) {
                Entry::Vacant(vacant) => {
                    vacant.insert(parent_id);
                }
                Entry::Occupied(occupied) => ensure!(
                    *occupied.get() == parent_id,
                    "scope instance {instance} has inconsistent parent attributions"
                ),
            }
        }
    }

    let mut direct: BTreeMap<&str, usize> = BTreeMap::new();
    for token in &carriers {
        let attributed = match &token.purpose {
            TokenPurpose::Normal => token.scope_stack.last(),
            TokenPurpose::ScopeLifecycle { stage, .. } => match stage {
                LifecycleStage::Entry => token.scope_stack.last(),
                LifecycleStage::Exit => token
                    .scope_stack
                    .len()
                    .checked_sub(2)
                    .and_then(|below_top| token.scope_stack.get(below_top)),
            },
        };
        if let Some(instance) = attributed {
            *direct.entry(instance.as_str()).or_insert(0) += 1;
        }
    }

    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (instance, parent_id) in &parent {
        if let Some(parent_id) = parent_id {
            children.entry(parent_id).or_default().push(instance);
        }
    }

    let mut memo: BTreeMap<&str, usize> = BTreeMap::new();
    for (instance_id, instance) in &envelope.scopes {
        let live = episode_count(instance_id.as_str(), &direct, &children, &mut memo);
        ensure!(
            instance.active_tokens == live,
            "scope instance {instance_id} stores {} active tokens but {live} live episodes \
             are attributable to it",
            instance.active_tokens,
        );
    }
    Ok(envelope.scopes.len())
}

// -------------------------------------------------------
// INV-K11 — join-policy semantics
// -------------------------------------------------------

fn join_fire_count(envelope: &ContinuationEnvelope) -> usize {
    envelope
        .events
        .iter()
        .filter(|event| {
            event.event_type == "transitionTaken" && event.node_id.as_deref() == Some("join")
        })
        .count()
}

/// Number of `effectObserved` events preceding the first firing of the join.
fn observations_before_first_fire(envelope: &ContinuationEnvelope) -> Option<usize> {
    let fire_sequence = envelope
        .events
        .iter()
        .find(|event| {
            event.event_type == "transitionTaken" && event.node_id.as_deref() == Some("join")
        })?
        .sequence;
    Some(
        envelope
            .events
            .iter()
            .filter(|event| event.event_type == "effectObserved" && event.sequence < fire_sequence)
            .count(),
    )
}

/// Drives a generated fork/join workflow in the scripted arrival order and
/// asserts the join policy's exact semantics: quorum fires at the k-th
/// arrival and never re-fires, race is won by the first arrival with late
/// arrivals following the declared policy, all/each/reduce complete with
/// their characteristic merged values, and no join state leaks.
pub(crate) fn check_join_semantics(case: &BranchingWorkflow) -> anyhow::Result<()> {
    let branch_count = case.branch_values.len();
    let outputs: BTreeMap<String, Value> = case
        .branch_values
        .iter()
        .enumerate()
        .map(|(branch, value)| (branch_node_id(branch), json!(value)))
        .collect();
    let observation_order: Vec<String> = case
        .observation_order
        .iter()
        .map(|&branch| branch_node_id(branch))
        .collect();
    let plan = DrivePlan {
        observation_order,
        output_plan: OutputPlan::PerNode(outputs),
        script: DriverScript {
            choices: Vec::new(),
            cancel_at: None,
        },
        checkpoint_at: None,
        max_iterations: 10_000,
    };
    let observed_value =
        |position: usize| json!(case.branch_values[case.observation_order[position]]);
    let observed_entry = |position: usize| {
        let branch = case.observation_order[position];
        (branch_label(branch), json!(case.branch_values[branch]))
    };

    let outcome = drive(&case.workflow, json!("prompt"), &plan);

    match &case.join {
        JoinKind::All => {
            let result = outcome.context("all-join run must complete")?;
            let expected: serde_json::Map<String, Value> =
                (0..branch_count).map(observed_entry).collect();
            ensure!(
                result.end == DriveEnd::Completed(Value::Object(expected)),
                "all-join must merge every producer label; got {:?}",
                result.end
            );
            ensure!(join_fire_count(&result.envelope) == 1, "all-join fires once");
            ensure!(
                observations_before_first_fire(&result.envelope) == Some(branch_count),
                "all-join fires only after every arrival"
            );
            ensure!(result.envelope.joins.is_empty(), "no join state leaks");
        }
        JoinKind::Each => {
            let result = outcome.context("each-join run must complete")?;
            let expected: Vec<Value> = (0..branch_count).map(observed_value).collect();
            ensure!(
                result.end == DriveEnd::Completed(Value::Array(expected)),
                "each-join passes every arrival through in order; got {:?}",
                result.end
            );
            ensure!(
                join_fire_count(&result.envelope) == branch_count,
                "each-join fires once per arrival"
            );
            ensure!(result.envelope.joins.is_empty(), "each-join keeps no state");
        }
        JoinKind::Quorum { quorum, late } => {
            let expected: serde_json::Map<String, Value> =
                (0..*quorum).map(observed_entry).collect();
            if *quorum == branch_count || *late == LateChoice::Discard {
                let result = outcome.context("quorum run must complete")?;
                ensure!(
                    result.end == DriveEnd::Completed(Value::Object(expected)),
                    "quorum merges exactly the first {quorum} arrivals; got {:?}",
                    result.end
                );
                ensure!(
                    join_fire_count(&result.envelope) == 1,
                    "a closed quorum join must never re-fire"
                );
                ensure!(
                    observations_before_first_fire(&result.envelope) == Some(*quorum),
                    "quorum fires exactly at the {quorum}-th arrival"
                );
                ensure!(result.envelope.joins.is_empty(), "no join state leaks");
            } else if *late == LateChoice::Cancel {
                let result = outcome.context("late-cancel quorum run terminates")?;
                ensure!(
                    result.end == DriveEnd::Cancelled,
                    "a late arrival under the cancel policy cancels the run; got {:?}",
                    result.end
                );
                ensure!(
                    observations_before_first_fire(&result.envelope) == Some(*quorum),
                    "the quorum still fired at the {quorum}-th arrival before cancellation"
                );
            } else {
                let error = match outcome {
                    Err(error) => error,
                    Ok(result) => bail!(
                        "a late arrival under the error policy must fail the run; got {:?}",
                        result.end
                    ),
                };
                ensure!(
                    error.to_string().contains("late arrival at closed join"),
                    "unexpected late-arrival error: {error:#}"
                );
            }
        }
        JoinKind::Race { late } => match late {
            LateChoice::Discard => {
                let result = outcome.context("race run must complete")?;
                ensure!(
                    result.end == DriveEnd::Completed(observed_value(0)),
                    "the first arrival wins the race; got {:?}",
                    result.end
                );
                ensure!(join_fire_count(&result.envelope) == 1, "a race fires once");
                ensure!(
                    observations_before_first_fire(&result.envelope) == Some(1),
                    "the race fires at the first arrival"
                );
                ensure!(result.envelope.joins.is_empty(), "no join state leaks");
            }
            LateChoice::Cancel => {
                let result = outcome.context("late-cancel race run terminates")?;
                ensure!(
                    result.end == DriveEnd::Cancelled,
                    "a late race arrival under the cancel policy cancels; got {:?}",
                    result.end
                );
            }
            LateChoice::Error => {
                let error = match outcome {
                    Err(error) => error,
                    Ok(result) => bail!(
                        "a late race arrival under the error policy must fail; got {:?}",
                        result.end
                    ),
                };
                ensure!(
                    error.to_string().contains("late arrival at closed join"),
                    "unexpected late-arrival error: {error:#}"
                );
            }
        },
        JoinKind::ReduceSum { threshold } => {
            let result = outcome.context("reduce run must complete")?;
            let DriveEnd::Completed(value) = &result.end else {
                bail!("reduce-join run must complete; got {:?}", result.end);
            };
            let total: i64 = case.branch_values.iter().sum();
            ensure!(*threshold == total, "generator wires threshold == total");
            #[allow(
                clippy::cast_precision_loss,
                reason = "branch sums stay far below 2^53"
            )]
            let expected = total as f64;
            ensure!(
                value.as_f64() == Some(expected),
                "reduce-sum merges the accumulator {expected}; got {value}"
            );
            ensure!(join_fire_count(&result.envelope) == 1, "reduce fired once");
            ensure!(
                observations_before_first_fire(&result.envelope) == Some(branch_count),
                "a total-sum threshold is reached exactly at the final arrival"
            );
            ensure!(result.envelope.joins.is_empty(), "no join state leaks");
        }
    }
    Ok(())
}

// -------------------------------------------------------
// INV-D8 / replay — canonical event order and determinism
// -------------------------------------------------------

pub(crate) fn envelope_bytes(envelope: &ContinuationEnvelope) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(envelope).context("envelope must serialize")
}

pub(crate) fn event_journal(envelope: &ContinuationEnvelope) -> anyhow::Result<String> {
    serde_json::to_string(&envelope.events).context("event journal must serialize")
}
