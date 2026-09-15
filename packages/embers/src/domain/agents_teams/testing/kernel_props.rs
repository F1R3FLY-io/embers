//! `proptest!` bindings of the generator strategies to the kernel invariant
//! checkers. Test names follow the shared invariant registry:
//! `fv_inv_<id>_<slug>`.

use proptest::prelude::*;
use serde_json::json;

use super::generators::{
    CycleKind,
    CycleWorkflow,
    DriverScript,
    JoinKind,
    LinearWorkflow,
    ScopedWorkflow,
    arb_bounded_cycle_workflow,
    arb_branching_workflow,
    arb_command_sequence,
    arb_json_value,
    arb_linear_workflow,
    arb_scoped_workflow,
    document,
    root_only_scopes,
};
use super::kernel_model::{
    DriveEnd,
    DrivePlan,
    OutputPlan,
    check_join_semantics,
    drive,
    envelope_bytes,
    event_journal,
};
use crate::domain::agents_teams::runtime::ContinuationEnvelope;
use crate::domain::agents_teams::workflow::WorkflowDefinition;

/// A drive case over any of the self-driving workflow shapes.
#[derive(Debug, Clone)]
enum TerminationCase {
    Linear(LinearWorkflow, DriverScript),
    Scoped(ScopedWorkflow, DriverScript),
}

fn arb_termination_case() -> impl Strategy<Value = TerminationCase> {
    prop_oneof![
        arb_linear_workflow().prop_flat_map(|linear| {
            arb_command_sequence(&linear.workflow)
                .prop_map(move |script| TerminationCase::Linear(linear.clone(), script))
        }),
        arb_scoped_workflow().prop_flat_map(|scoped| {
            arb_command_sequence(&scoped.workflow)
                .prop_map(move |script| TerminationCase::Scoped(scoped.clone(), script))
        }),
    ]
}

impl TerminationCase {
    fn workflow(&self) -> &WorkflowDefinition {
        match self {
            Self::Linear(linear, _) => &linear.workflow,
            Self::Scoped(scoped, _) => &scoped.workflow,
        }
    }

    fn script(&self) -> &DriverScript {
        match self {
            Self::Linear(_, script) | Self::Scoped(_, script) => script,
        }
    }
}

fn cycle_plan(case: &CycleWorkflow) -> DrivePlan {
    let mut plan = DrivePlan::scripted(DriverScript {
        choices: Vec::new(),
        cancel_at: None,
    });
    if matches!(case.kind, CycleKind::DurableYield { .. }) {
        plan.output_plan = OutputPlan::Counter;
    }
    plan
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// INV-K1 (`CounterFaithful`): every scope instance's stored
    /// `active_tokens` counter equals the live population recounted from the
    /// envelope, at every quiescent step of a scoped run. The sample count
    /// guards against vacuity: a run that never checked a live instance
    /// fails.
    #[test]
    fn fv_inv_k1_token_conservation(
        scoped in arb_scoped_workflow(),
        script in super::generators::arb_driver_script(),
    ) {
        let cancel_free = script.cancel_at.is_none();
        let result = drive(
            &scoped.workflow,
            json!({"request": 1}),
            &DrivePlan::scripted(script),
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => return Err(TestCaseError::fail(format!(
                "scoped run must uphold conservation and terminate: {error:#}"
            ))),
        };
        if cancel_free {
            prop_assert!(
                result.conservation_samples > 0,
                "a scoped run must actually exercise the conservation checker"
            );
            prop_assert!(
                matches!(result.end, DriveEnd::Completed(_)),
                "a cancel-free scoped run completes; got {:?}",
                result.end
            );
            let expect_observed = |node: &str| {
                result.observed_nodes.iter().any(|observed| observed == node)
            };
            if scoped.lifecycle {
                prop_assert!(
                    expect_observed("entry") && expect_observed("exit"),
                    "lifecycle scopes must run their entry and exit activities; observed {:?}",
                    result.observed_nodes
                );
            }
            match scoped.shape {
                super::generators::ScopeShape::Compound { nested, .. } => {
                    prop_assert!(expect_observed("c0"), "the compound chain must run");
                    if nested {
                        prop_assert!(
                            expect_observed("g0"),
                            "the nested scope's region must run; observed {:?}",
                            result.observed_nodes
                        );
                    }
                }
                super::generators::ScopeShape::Parallel { regions } => {
                    for region in 0..regions {
                        prop_assert!(
                            expect_observed(&format!("r{region}")),
                            "parallel region {} must run; observed {:?}",
                            region,
                            result.observed_nodes
                        );
                    }
                }
            }
        }
    }

    /// INV-K14 (termination): a valid definition driven by any script
    /// reaches `Completed` or `Cancelled` within the driver bound — it never
    /// diverges and never panics.
    #[test]
    fn fv_inv_k14_no_divergence(case in arb_termination_case()) {
        let result = drive(
            case.workflow(),
            json!({"prompt": "steady"}),
            &DrivePlan::scripted(case.script().clone()),
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => return Err(TestCaseError::fail(format!(
                "run must reach a terminal without diverging: {error:#}"
            ))),
        };
        if let TerminationCase::Linear(linear, script) = &case
            && script.choices.is_empty()
            && script.cancel_at.is_none()
        {
            // The canonical all-zero script approves every gate and answers
            // every guard true, so the whole chain runs: the run completes
            // and observes exactly the generated activity steps.
            prop_assert!(
                matches!(result.end, DriveEnd::Completed(_)),
                "a canonical linear drive completes; got {:?}",
                result.end
            );
            let activity_steps = linear
                .steps
                .iter()
                .filter(|step| matches!(step, super::generators::LinearStep::Activity))
                .count();
            prop_assert_eq!(
                result.observed_nodes.len(),
                activity_steps,
                "every generated activity step is invoked exactly once"
            );
        }
    }

    /// INV-K14 (bounded-cycle exhaustion): bounded and durable-yield cycles
    /// execute exactly their permitted number of iterations and then
    /// complete with the value the final iteration produced.
    #[test]
    fn fv_inv_k14_bounded_cycles_exhaust(
        case in arb_bounded_cycle_workflow(),
        input in arb_json_value(),
    ) {
        let result = drive(&case.workflow, input.clone(), &cycle_plan(&case));
        let result = match result {
            Ok(result) => result,
            Err(error) => return Err(TestCaseError::fail(format!(
                "bounded cycle must terminate: {error:#}"
            ))),
        };
        match case.kind {
            CycleKind::BoundedGateway { .. } => {
                prop_assert_eq!(
                    result.end,
                    DriveEnd::Completed(input),
                    "a pure bounded gateway cycle passes the input through"
                );
            }
            CycleKind::BoundedActivity { max_iterations } => {
                let invocations = usize::try_from(max_iterations)
                    .expect("bounded iterations fit usize") + 1;
                prop_assert_eq!(
                    result.observed_nodes.len(),
                    invocations,
                    "maxIterations {} admits exactly {} activity invocations",
                    max_iterations,
                    invocations
                );
                prop_assert_eq!(result.end, DriveEnd::Completed(input));
            }
            CycleKind::DurableYield { limit } => {
                let invocations = usize::try_from(limit)
                    .expect("yield limits fit usize");
                prop_assert_eq!(
                    result.observed_nodes.len(),
                    invocations,
                    "the counter guard stops the durable loop at {}",
                    limit
                );
                prop_assert_eq!(
                    result.end,
                    DriveEnd::Completed(json!({"counter": limit}))
                );
            }
        }
    }

    /// INV-K11 (quorum): the join fires exactly at the k-th arrival, never
    /// re-fires, merges exactly the first k producers, and each late arrival
    /// follows the declared policy.
    #[test]
    fn fv_inv_k11_quorum_fires_at_k(
        case in arb_branching_workflow()
            .prop_filter("quorum joins only", |case| {
                matches!(case.join, JoinKind::Quorum { .. })
            }),
    ) {
        let checked = check_join_semantics(&case);
        prop_assert!(checked.is_ok(), "{:#}", checked.expect_err("checked above"));
    }

    /// INV-K11 (race): the first arrival wins and later arrivals follow the
    /// declared late-arrival policy (discard / cancel / error).
    #[test]
    fn fv_inv_k11_race_first_arrival_wins(
        case in arb_branching_workflow()
            .prop_filter("race joins only", |case| {
                matches!(case.join, JoinKind::Race { .. })
            }),
    ) {
        let checked = check_join_semantics(&case);
        prop_assert!(checked.is_ok(), "{:#}", checked.expect_err("checked above"));
    }

    /// INV-K11 (all / each / reduce): the structural joins complete with
    /// their characteristic merged values and leak no join state.
    #[test]
    fn fv_inv_k11_all_each_reduce_complete(
        case in arb_branching_workflow()
            .prop_filter("structural joins only", |case| {
                matches!(
                    case.join,
                    JoinKind::All | JoinKind::Each | JoinKind::ReduceSum { .. }
                )
            }),
    ) {
        let checked = check_join_semantics(&case);
        prop_assert!(checked.is_ok(), "{:#}", checked.expect_err("checked above"));
    }

    /// INV-D8: the committed event journal is a deterministic function of the
    /// definition, the input, and the command sequence — two identical drives
    /// commit identical journals.
    #[test]
    fn fv_inv_d8_event_order_deterministic(
        case in arb_termination_case(),
        input in arb_json_value(),
    ) {
        let plan = DrivePlan::scripted(case.script().clone());
        let first = drive(case.workflow(), input.clone(), &plan);
        let second = drive(case.workflow(), input, &plan);
        let (first, second) = match (first, second) {
            (Ok(first), Ok(second)) => (first, second),
            (first, second) => return Err(TestCaseError::fail(format!(
                "both drives must terminate: {first:?} vs {second:?}"
            ))),
        };
        let first_journal = event_journal(&first.envelope)
            .expect("event journal serializes");
        let second_journal = event_journal(&second.envelope)
            .expect("event journal serializes");
        prop_assert_eq!(first_journal, second_journal);
    }

    /// Deterministic replay: identical inputs yield byte-identical serialized
    /// envelopes, and resuming from a mid-run persisted envelope reaches the
    /// same final state as running straight through.
    #[test]
    fn fv_inv_replay_deterministic(
        case in arb_termination_case(),
        input in arb_json_value(),
        checkpoint in 0_usize..8,
    ) {
        let straight_plan = DrivePlan::scripted(case.script().clone());
        let mut checkpoint_plan = straight_plan.clone();
        checkpoint_plan.checkpoint_at = Some(checkpoint);

        let first = drive(case.workflow(), input.clone(), &straight_plan);
        let second = drive(case.workflow(), input.clone(), &straight_plan);
        let resumed = drive(case.workflow(), input, &checkpoint_plan);
        let (first, second, resumed) = match (first, second, resumed) {
            (Ok(first), Ok(second), Ok(resumed)) => (first, second, resumed),
            (first, second, resumed) => return Err(TestCaseError::fail(format!(
                "all drives must terminate: {first:?} vs {second:?} vs {resumed:?}"
            ))),
        };
        let first_bytes = envelope_bytes(&first.envelope).expect("envelope serializes");
        let second_bytes = envelope_bytes(&second.envelope).expect("envelope serializes");
        let resumed_bytes = envelope_bytes(&resumed.envelope).expect("envelope serializes");
        prop_assert_eq!(
            &first_bytes,
            &second_bytes,
            "two identical drives must serialize identically"
        );
        prop_assert_eq!(
            &first_bytes,
            &resumed_bytes,
            "resuming from a persisted mid-run envelope must reach the same final state"
        );
    }
}

/// INV-K14 (step budget): a hot loop that validation would reject cannot spin
/// the kernel forever — `run_until_blocked` stops it with the documented
/// 1,000,000-microstep budget error instead of diverging.
#[test]
fn fv_inv_k14_step_budget_stops_hot_loops() {
    let hot = document(
        json!({
            "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
            "work": {"id": "work", "label": "Work", "scopeId": "root", "kind": "gateway"}
        }),
        json!({
            "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
            "spin": {"id": "spin", "mode": "allMatches", "noMatch": "fail"}
        }),
        json!({
            "start": {"id": "start", "sourceNodeId": "in", "targetNodeId": "work",
                      "trigger": "completed", "priority": 0, "routeGroupId": "start"},
            "spin": {"id": "spin", "sourceNodeId": "work", "targetNodeId": "work",
                     "trigger": "completed", "priority": 0, "routeGroupId": "spin"}
        }),
        root_only_scopes("in"),
    );
    let workflow = WorkflowDefinition::new(hot).expect("a hot loop is still a valid draft");
    workflow
        .parsed()
        .validate_publishable()
        .expect_err("publication must reject the hot loop");

    let mut envelope = ContinuationEnvelope::start(
        workflow.parsed(),
        "run".into(),
        "rho:id:team".into(),
        workflow.definition_hash(),
        json!("fuel"),
    )
    .expect("continuation should start");
    let error = envelope
        .run_until_blocked(workflow.parsed(), chrono::DateTime::<chrono::Utc>::UNIX_EPOCH)
        .expect_err("a hot loop must exhaust the step budget, not diverge");
    assert!(
        error.to_string().contains("step budget exhausted"),
        "unexpected budget error: {error:#}"
    );
}
