//! Shrink-friendly proptest strategies over valid Workflow Definition v2
//! documents, mirroring the JSON scaffolding the `runtime.rs` and
//! `workflow.rs` test modules build by hand.
//!
//! Every strategy constructs the real [`WorkflowDefinition`] through the real
//! `from_json` path, so a generated case exercises exactly the artifacts the
//! kernel executes in production. Sizes are bounded (≤ 8 nodes) to keep
//! shrinking effective.

use proptest::prelude::*;
use serde_json::{Value, json};

use crate::domain::agents_teams::workflow::WorkflowDefinition;

/// Upper bound on chain length / branch width so cases stay shrinkable.
const MAX_LINEAR_STEPS: usize = 5;
const MAX_BRANCHES: usize = 4;

/// The full document scaffold shared by every generated workflow, identical in
/// shape to the `definition()` helper in the `runtime.rs` test module.
pub(crate) fn document(
    nodes: Value,
    groups: Value,
    transitions: Value,
    scopes: Value,
) -> Value {
    let mut value = json!({
        "schemaVersion": "2.0",
        "entryScopeId": "root",
        "interface": {
            "inputs": {"input": {"kind": "legacy"}},
            "outputs": {"output": {"kind": "legacy"}}
        },
        "nodes": {},
        "policies": {"defaultExecution": "durable", "visibility": "private"},
        "presentation": {"edges": {}, "nodes": {}},
        "routeGroups": {},
        "scopes": {},
        "transitions": {},
        "types": {}
    });
    value["nodes"] = nodes;
    value["routeGroups"] = groups;
    value["scopes"] = scopes;
    value["transitions"] = transitions;
    value
}

pub(crate) fn root_only_scopes(initial: &str) -> Value {
    json!({
        "root": {
            "id": "root", "label": "Root", "kind": "root",
            "initialNodeIds": [initial],
            "entryActivities": [], "exitActivities": [],
            "history": "none"
        }
    })
}

fn parse(document: Value) -> WorkflowDefinition {
    WorkflowDefinition::new(document).expect("generated workflow document must parse")
}

fn parse_publishable(document: Value) -> WorkflowDefinition {
    let definition = parse(document);
    definition
        .parsed()
        .validate_publishable()
        .expect("generated workflow must be publishable");
    definition
}

// -------------------------------------------------------
// Linear workflows
// -------------------------------------------------------

/// One interior step of a linear chain. Every variant self-drives through the
/// `kernel_model` driver: approvals route both `completed` and `rejected`
/// forward, guarded gateways complete on no-match, waits resume through the
/// matching command.
#[derive(Debug, Clone)]
pub(crate) enum LinearStep {
    Gateway,
    Activity,
    TimerWait { delay_millis: u64 },
    SignalWait { name: String },
    EventWait { name: String },
    Approval { quorum: usize, approvers: Vec<String> },
    /// A gateway whose sole outgoing transition carries a pure Rholang guard,
    /// so routing suspends on a [`super::super::runtime::GuardRequest`]. The
    /// route group completes on no-match, so a `false` observation still
    /// terminates the run.
    RholangGuardGateway,
}

#[derive(Debug, Clone)]
pub(crate) struct LinearWorkflow {
    pub workflow: WorkflowDefinition,
    pub steps: Vec<LinearStep>,
}

fn linear_step() -> impl Strategy<Value = LinearStep> {
    prop_oneof![
        3 => Just(LinearStep::Gateway),
        3 => Just(LinearStep::Activity),
        1 => (1_u64..=10_000).prop_map(|delay_millis| LinearStep::TimerWait { delay_millis }),
        1 => "[a-z]{1,6}".prop_map(|name| LinearStep::SignalWait { name }),
        1 => "[a-z]{1,6}".prop_map(|name| LinearStep::EventWait { name }),
        1 => (1_usize..=3).prop_flat_map(|approver_count| {
            (1_usize..=approver_count).prop_map(move |quorum| LinearStep::Approval {
                quorum,
                approvers: ["alice", "bob", "carol"][..approver_count]
                    .iter()
                    .map(|approver| (*approver).to_owned())
                    .collect(),
            })
        }),
        1 => Just(LinearStep::RholangGuardGateway),
    ]
}

fn linear_document(steps: &[LinearStep]) -> Value {
    let mut nodes = serde_json::Map::new();
    let mut groups = serde_json::Map::new();
    let mut transitions = serde_json::Map::new();

    nodes.insert(
        "in".into(),
        json!({"id": "in", "label": "Input", "scopeId": "root", "kind": "input"}),
    );
    nodes.insert(
        "out".into(),
        json!({"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}),
    );

    let node_id = |index: usize| format!("n{index}");
    for (index, step) in steps.iter().enumerate() {
        let id = node_id(index);
        let label = format!("Step {index}");
        let node = match step {
            LinearStep::Gateway | LinearStep::RholangGuardGateway => {
                json!({"id": id, "label": label, "scopeId": "root", "kind": "gateway"})
            }
            LinearStep::Activity => json!({
                "id": id, "label": label, "scopeId": "root", "kind": "activity",
                "activity": {"kind": "builtin", "name": "compress"}
            }),
            LinearStep::TimerWait { delay_millis } => json!({
                "id": id, "label": label, "scopeId": "root", "kind": "wait",
                "waitFor": "timer", "timer": {"delayMillis": delay_millis}
            }),
            LinearStep::SignalWait { name } => json!({
                "id": id, "label": label, "scopeId": "root", "kind": "wait",
                "waitFor": "signal", "signal": name
            }),
            LinearStep::EventWait { name } => json!({
                "id": id, "label": label, "scopeId": "root", "kind": "wait",
                "waitFor": "event", "signal": name
            }),
            LinearStep::Approval { quorum, approvers } => json!({
                "id": id, "label": label, "scopeId": "root", "kind": "approval",
                "policyId": format!("policy-{index}"), "quorum": quorum,
                "approvers": approvers
            }),
        };
        nodes.insert(id, node);
    }

    // Chain transitions: in -> n0 -> ... -> out, one route group per
    // (source, trigger) pair as `validate_publishable` requires.
    let mut link = |source: &str, target: &str, trigger: &str, guard: Option<Value>| {
        let group_id = format!("g:{source}:{trigger}");
        let no_match = if guard.is_some() { "complete" } else { "fail" };
        groups.insert(
            group_id.clone(),
            json!({"id": group_id, "mode": "allMatches", "noMatch": no_match}),
        );
        let transition_id = format!("t:{source}:{trigger}");
        let mut transition = json!({
            "id": transition_id,
            "sourceNodeId": source,
            "targetNodeId": target,
            "trigger": trigger,
            "priority": 0,
            "routeGroupId": group_id
        });
        if let Some(guard) = guard {
            transition["guard"] = guard;
        }
        transitions.insert(transition_id, transition);
    };

    let mut previous = "in".to_owned();
    for index in 0..steps.len() {
        let id = node_id(index);
        link(&previous, &id, "completed", None);
        previous = id;
    }
    let rholang_guard = json!({
        "kind": "rholang",
        "inputs": {"released": {"root": "output", "path": []}},
        "source": "$released == $released"
    });
    let final_guard = match steps.last() {
        Some(LinearStep::RholangGuardGateway) => Some(rholang_guard.clone()),
        _ => None,
    };
    link(&previous, "out", "completed", final_guard);
    for (index, step) in steps.iter().enumerate() {
        let id = node_id(index);
        match step {
            LinearStep::Approval { .. } => {
                let target = steps
                    .get(index + 1)
                    .map_or_else(|| "out".to_owned(), |_| node_id(index + 1));
                link(&id, &target, "rejected", None);
            }
            LinearStep::RholangGuardGateway if index + 1 < steps.len() => {
                // Interior guarded gateways: replace the plain completed edge
                // guard by regenerating it with the Rholang guard attached.
                let target = node_id(index + 1);
                link(&id, &target, "completed", Some(rholang_guard.clone()));
            }
            _ => {}
        }
    }

    document(
        Value::Object(nodes),
        Value::Object(groups),
        Value::Object(transitions),
        root_only_scopes("in"),
    )
}

pub(crate) fn arb_linear_workflow() -> impl Strategy<Value = LinearWorkflow> {
    proptest::collection::vec(linear_step(), 0..=MAX_LINEAR_STEPS).prop_map(|steps| {
        LinearWorkflow {
            workflow: parse_publishable(linear_document(&steps)),
            steps,
        }
    })
}

// -------------------------------------------------------
// Branching workflows (fork through every join policy)
// -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LateChoice {
    Discard,
    Cancel,
    Error,
}

impl LateChoice {
    const fn as_json(self) -> &'static str {
        match self {
            Self::Discard => "discard",
            Self::Cancel => "cancel",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum JoinKind {
    All,
    Each,
    Quorum { quorum: usize, late: LateChoice },
    Race { late: LateChoice },
    /// Sum-reduce that fires once the accumulator reaches the total of every
    /// branch value, i.e. exactly at the final arrival.
    ReduceSum { threshold: i64 },
}

#[derive(Debug, Clone)]
pub(crate) struct BranchingWorkflow {
    pub workflow: WorkflowDefinition,
    pub join: JoinKind,
    /// The integer each branch activity reports, indexed by branch.
    pub branch_values: Vec<i64>,
    /// The order (by branch index) in which the driver observes branch
    /// activity effects — arrival order at the join.
    pub observation_order: Vec<usize>,
}

pub(crate) fn branch_node_id(branch: usize) -> String {
    format!("branch{branch}")
}

pub(crate) fn branch_label(branch: usize) -> String {
    format!("B{branch}")
}

fn branching_document(branch_count: usize, join: &Value) -> Value {
    let mut nodes = serde_json::Map::new();
    let mut groups = serde_json::Map::new();
    let mut transitions = serde_json::Map::new();

    nodes.insert(
        "in".into(),
        json!({"id": "in", "label": "Input", "scopeId": "root", "kind": "input"}),
    );
    nodes.insert(
        "join".into(),
        json!({"id": "join", "label": "Join", "scopeId": "root", "join": join, "kind": "gateway"}),
    );
    nodes.insert(
        "out".into(),
        json!({"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}),
    );
    groups.insert(
        "fork".into(),
        json!({"id": "fork", "mode": "allMatches", "noMatch": "fail"}),
    );
    groups.insert(
        "done".into(),
        json!({"id": "done", "mode": "allMatches", "noMatch": "fail"}),
    );
    transitions.insert(
        "join-out".into(),
        json!({
            "id": "join-out", "sourceNodeId": "join", "targetNodeId": "out",
            "trigger": "completed", "priority": 0, "routeGroupId": "done"
        }),
    );

    for branch in 0..branch_count {
        let id = branch_node_id(branch);
        nodes.insert(
            id.clone(),
            json!({
                "id": id, "label": branch_label(branch), "scopeId": "root",
                "kind": "activity",
                "activity": {"kind": "builtin", "name": "compress"}
            }),
        );
        groups.insert(
            id.clone(),
            json!({"id": id, "mode": "allMatches", "noMatch": "fail"}),
        );
        transitions.insert(
            format!("to-{id}"),
            json!({
                "id": format!("to-{id}"), "sourceNodeId": "in", "targetNodeId": id,
                "trigger": "completed", "priority": branch, "routeGroupId": "fork"
            }),
        );
        transitions.insert(
            format!("{id}-join"),
            json!({
                "id": format!("{id}-join"), "sourceNodeId": id, "targetNodeId": "join",
                "trigger": "completed", "priority": 0, "routeGroupId": id
            }),
        );
    }

    document(
        Value::Object(nodes),
        Value::Object(groups),
        Value::Object(transitions),
        root_only_scopes("in"),
    )
}

fn join_json(join: &JoinKind) -> Value {
    let correlation = json!({"kind": "root"});
    match join {
        JoinKind::All => json!({"mode": "all", "correlation": correlation}),
        JoinKind::Each => json!({"mode": "each", "correlation": correlation}),
        JoinKind::Quorum { quorum, late } => json!({
            "mode": "quorum", "quorum": quorum,
            "late": late.as_json(), "correlation": correlation
        }),
        JoinKind::Race { late } => json!({
            "mode": "race", "late": late.as_json(), "correlation": correlation
        }),
        JoinKind::ReduceSum { threshold } => json!({
            "mode": "reduce",
            "reducer": {"operation": "sum"},
            "until": {
                "kind": "compare",
                "left": {"kind": "path", "root": "output", "path": []},
                "operator": "ge",
                "right": {"kind": "literal", "value": threshold}
            },
            "correlation": correlation
        }),
    }
}

fn late_choice() -> impl Strategy<Value = LateChoice> {
    prop_oneof![
        Just(LateChoice::Discard),
        Just(LateChoice::Cancel),
        Just(LateChoice::Error),
    ]
}

fn arb_branching_parts() -> impl Strategy<Value = (Vec<i64>, Vec<usize>, JoinKind)> {
    (2_usize..=MAX_BRANCHES).prop_flat_map(|branch_count| {
        (
            proptest::collection::vec(1_i64..=9, branch_count),
            Just((0..branch_count).collect::<Vec<_>>()).prop_shuffle(),
            join_kind(branch_count),
        )
    })
}

fn join_kind(branch_count: usize) -> BoxedStrategy<JoinKind> {
    prop_oneof![
        Just(JoinKind::All),
        Just(JoinKind::Each),
        (1..=branch_count, late_choice())
            .prop_map(|(quorum, late)| JoinKind::Quorum { quorum, late }),
        late_choice().prop_map(|late| JoinKind::Race { late }),
        Just(JoinKind::ReduceSum { threshold: 0 }),
    ]
    .boxed()
}

pub(crate) fn arb_branching_workflow() -> impl Strategy<Value = BranchingWorkflow> {
    arb_branching_parts().prop_map(|(branch_values, observation_order, mut join)| {
        if let JoinKind::ReduceSum { threshold } = &mut join {
            *threshold = branch_values.iter().sum();
        }
        let workflow =
            parse_publishable(branching_document(branch_values.len(), &join_json(&join)));
        BranchingWorkflow {
            workflow,
            join,
            branch_values,
            observation_order,
        }
    })
}

// -------------------------------------------------------
// Bounded cycles (publishable) and hot cycles (rejected)
// -------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(crate) enum CycleKind {
    /// Pure gateway self-loop bounded by `maxIterations`; self-driving.
    BoundedGateway { max_iterations: u64 },
    /// Activity self-loop bounded by `maxIterations`; each iteration mints a
    /// distinct effect key.
    BoundedActivity { max_iterations: u64 },
    /// Activity loop bounded by a counter guard whose backward edge is a
    /// durable yield, so every iteration parks and resumes by timer.
    DurableYield { limit: i64 },
}

#[derive(Debug, Clone)]
pub(crate) struct CycleWorkflow {
    pub workflow: WorkflowDefinition,
    pub kind: CycleKind,
}

fn cycle_document(kind: CycleKind) -> Value {
    match kind {
        CycleKind::BoundedGateway { max_iterations } => {
            let nodes = json!({
                "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
                "loop": {"id": "loop", "label": "Loop", "scopeId": "root", "kind": "gateway"},
                "out": {"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}
            });
            let groups = json!({
                "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
                "again": {"id": "again", "mode": "allMatches", "noMatch": "fail"},
                "limit": {"id": "limit", "mode": "allMatches", "noMatch": "fail"}
            });
            let transitions = json!({
                "start": {"id": "start", "sourceNodeId": "in", "targetNodeId": "loop",
                          "trigger": "completed", "priority": 0, "routeGroupId": "start"},
                "again": {"id": "again", "sourceNodeId": "loop", "targetNodeId": "loop",
                          "trigger": "completed", "priority": 0, "routeGroupId": "again",
                          "loop": {"maxIterations": max_iterations}},
                "done": {"id": "done", "sourceNodeId": "loop", "targetNodeId": "out",
                         "trigger": "iterationLimit", "priority": 0, "routeGroupId": "limit"}
            });
            document(nodes, groups, transitions, root_only_scopes("in"))
        }
        CycleKind::BoundedActivity { max_iterations } => {
            let nodes = json!({
                "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
                "act": {"id": "act", "label": "Act", "scopeId": "root", "kind": "activity",
                        "activity": {"kind": "builtin", "name": "compress"}},
                "out": {"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}
            });
            let groups = json!({
                "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
                "again": {"id": "again", "mode": "allMatches", "noMatch": "fail"},
                "limit": {"id": "limit", "mode": "allMatches", "noMatch": "fail"}
            });
            let transitions = json!({
                "start": {"id": "start", "sourceNodeId": "in", "targetNodeId": "act",
                          "trigger": "completed", "priority": 0, "routeGroupId": "start"},
                "again": {"id": "again", "sourceNodeId": "act", "targetNodeId": "act",
                          "trigger": "completed", "priority": 0, "routeGroupId": "again",
                          "loop": {"maxIterations": max_iterations}},
                "done": {"id": "done", "sourceNodeId": "act", "targetNodeId": "out",
                         "trigger": "iterationLimit", "priority": 0, "routeGroupId": "limit"}
            });
            document(nodes, groups, transitions, root_only_scopes("in"))
        }
        CycleKind::DurableYield { limit } => {
            let nodes = json!({
                "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
                "act": {"id": "act", "label": "Act", "scopeId": "root", "kind": "activity",
                        "activity": {"kind": "builtin", "name": "compress"}},
                "out": {"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}
            });
            let groups = json!({
                "start": {"id": "start", "mode": "allMatches", "noMatch": "fail"},
                "route": {"id": "route", "mode": "allMatches", "noMatch": "fail"}
            });
            let transitions = json!({
                "start": {"id": "start", "sourceNodeId": "in", "targetNodeId": "act",
                          "trigger": "completed", "priority": 0, "routeGroupId": "start"},
                "again": {"id": "again", "sourceNodeId": "act", "targetNodeId": "act",
                          "trigger": "completed", "priority": 0, "routeGroupId": "route",
                          "loop": {"durableYield": true},
                          "guard": {"kind": "compare",
                                    "left": {"kind": "path", "root": "output", "path": ["counter"]},
                                    "operator": "lt",
                                    "right": {"kind": "literal", "value": limit}}},
                "done": {"id": "done", "sourceNodeId": "act", "targetNodeId": "out",
                         "trigger": "completed", "priority": 1, "routeGroupId": "route",
                         "guard": {"kind": "compare",
                                   "left": {"kind": "path", "root": "output", "path": ["counter"]},
                                   "operator": "ge",
                                   "right": {"kind": "literal", "value": limit}}}
            });
            document(nodes, groups, transitions, root_only_scopes("in"))
        }
    }
}

pub(crate) fn arb_bounded_cycle_workflow() -> impl Strategy<Value = CycleWorkflow> {
    prop_oneof![
        (1_u64..=3).prop_map(|max_iterations| CycleKind::BoundedGateway { max_iterations }),
        (1_u64..=3).prop_map(|max_iterations| CycleKind::BoundedActivity { max_iterations }),
        (1_i64..=3).prop_map(|limit| CycleKind::DurableYield { limit }),
    ]
    .prop_map(|kind| CycleWorkflow {
        workflow: parse_publishable(cycle_document(kind)),
        kind,
    })
}

/// A ring of gateways with no progress boundary, plus the two repaired
/// variants (a bounded edge, a wait node) publication must accept.
#[derive(Debug, Clone)]
pub(crate) struct HotCycleCase {
    /// The offending document: `validate_publishable` must reject it.
    pub hot: Value,
    /// The same ring with `loop: {maxIterations}` on one cycle edge.
    pub bounded_fix: Value,
    /// The same ring with one gateway replaced by a timer wait node.
    pub wait_fix: Value,
}

fn ring_document(ring_size: usize, repair: Option<RingRepair>) -> Value {
    let mut nodes = serde_json::Map::new();
    let mut groups = serde_json::Map::new();
    let mut transitions = serde_json::Map::new();

    nodes.insert(
        "in".into(),
        json!({"id": "in", "label": "Input", "scopeId": "root", "kind": "input"}),
    );
    for index in 0..ring_size {
        let id = format!("ring{index}");
        let node = match repair {
            Some(RingRepair::WaitNode { node }) if node == index => json!({
                "id": id, "label": format!("Ring {index}"), "scopeId": "root",
                "kind": "wait", "waitFor": "timer", "timer": {"delayMillis": 1000}
            }),
            _ => json!({
                "id": id, "label": format!("Ring {index}"), "scopeId": "root",
                "kind": "gateway"
            }),
        };
        nodes.insert(id, node);
    }

    groups.insert(
        "enter".into(),
        json!({"id": "enter", "mode": "allMatches", "noMatch": "fail"}),
    );
    transitions.insert(
        "enter".into(),
        json!({
            "id": "enter", "sourceNodeId": "in", "targetNodeId": "ring0",
            "trigger": "completed", "priority": 0, "routeGroupId": "enter"
        }),
    );
    for index in 0..ring_size {
        let source = format!("ring{index}");
        let target = format!("ring{}", (index + 1) % ring_size);
        let group_id = format!("g{index}");
        groups.insert(
            group_id.clone(),
            json!({"id": group_id, "mode": "allMatches", "noMatch": "fail"}),
        );
        let mut transition = json!({
            "id": format!("e{index}"), "sourceNodeId": source, "targetNodeId": target,
            "trigger": "completed", "priority": 0, "routeGroupId": group_id
        });
        if let Some(RingRepair::BoundedEdge {
            edge,
            max_iterations,
        }) = repair
            && edge == index
        {
            transition["loop"] = json!({"maxIterations": max_iterations});
        }
        transitions.insert(format!("e{index}"), transition);
    }

    document(
        Value::Object(nodes),
        Value::Object(groups),
        Value::Object(transitions),
        root_only_scopes("in"),
    )
}

#[derive(Debug, Clone, Copy)]
enum RingRepair {
    BoundedEdge { edge: usize, max_iterations: u64 },
    WaitNode { node: usize },
}

pub(crate) fn arb_hot_cycle_workflow() -> impl Strategy<Value = HotCycleCase> {
    (1_usize..=4)
        .prop_flat_map(|ring_size| {
            (
                Just(ring_size),
                0..ring_size,
                1_u64..=5,
            )
        })
        .prop_map(|(ring_size, repaired_index, max_iterations)| HotCycleCase {
            hot: ring_document(ring_size, None),
            bounded_fix: ring_document(
                ring_size,
                Some(RingRepair::BoundedEdge {
                    edge: repaired_index,
                    max_iterations,
                }),
            ),
            wait_fix: ring_document(
                ring_size,
                Some(RingRepair::WaitNode {
                    node: repaired_index,
                }),
            ),
        })
}

// -------------------------------------------------------
// Scoped workflows (compound / parallel, lifecycle activities)
// -------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(crate) enum ScopeShape {
    /// One compound scope whose chain has `chain_length` activities before
    /// its final node, optionally nesting a second compound scope.
    Compound { chain_length: usize, nested: bool },
    /// One parallel scope with `regions` concurrent activity regions.
    Parallel { regions: usize },
}

#[derive(Debug, Clone)]
pub(crate) struct ScopedWorkflow {
    pub workflow: WorkflowDefinition,
    pub shape: ScopeShape,
    /// Whether the child scope declares an entry and an exit activity.
    pub lifecycle: bool,
}

fn scoped_document(shape: ScopeShape, lifecycle: bool) -> Value {
    let mut nodes = serde_json::Map::new();
    let mut groups = serde_json::Map::new();
    let mut transitions = serde_json::Map::new();
    let mut scopes = serde_json::Map::new();

    nodes.insert(
        "in".into(),
        json!({"id": "in", "label": "Input", "scopeId": "root", "kind": "input"}),
    );
    nodes.insert(
        "scope".into(),
        json!({"id": "scope", "label": "Scope", "scopeId": "root",
               "kind": "scope", "childScopeId": "child"}),
    );
    nodes.insert(
        "out".into(),
        json!({"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}),
    );
    scopes.insert(
        "root".into(),
        json!({"id": "root", "label": "Root", "kind": "root",
               "initialNodeIds": ["in"], "entryActivities": [],
               "exitActivities": [], "history": "none"}),
    );

    let link = |transitions: &mut serde_json::Map<String, Value>,
                groups: &mut serde_json::Map<String, Value>,
                source: &str,
                target: &str| {
        let group_id = format!("g:{source}");
        groups.insert(
            group_id.clone(),
            json!({"id": group_id, "mode": "allMatches", "noMatch": "fail"}),
        );
        let transition_id = format!("t:{source}");
        transitions.insert(
            transition_id.clone(),
            json!({
                "id": transition_id, "sourceNodeId": source, "targetNodeId": target,
                "trigger": "completed", "priority": 0, "routeGroupId": group_id
            }),
        );
    };
    link(&mut transitions, &mut groups, "in", "scope");
    link(&mut transitions, &mut groups, "scope", "out");

    let activity = |id: &str, scope: &str| {
        json!({
            "id": id, "label": id.to_uppercase(), "scopeId": scope, "kind": "activity",
            "activity": {"kind": "builtin", "name": "compress"}
        })
    };

    let (entry_activities, exit_activities) = if lifecycle {
        nodes.insert("entry".into(), activity("entry", "child"));
        nodes.insert("exit".into(), activity("exit", "child"));
        (json!(["entry"]), json!(["exit"]))
    } else {
        (json!([]), json!([]))
    };

    match shape {
        ScopeShape::Compound {
            chain_length,
            nested,
        } => {
            let mut previous = "c0".to_owned();
            nodes.insert("c0".into(), activity("c0", "child"));
            for index in 1..chain_length {
                let id = format!("c{index}");
                nodes.insert(id.clone(), activity(&id, "child"));
                link(&mut transitions, &mut groups, &previous, &id);
                previous = id;
            }
            if nested {
                nodes.insert(
                    "inner".into(),
                    json!({"id": "inner", "label": "Inner", "scopeId": "child",
                           "kind": "scope", "childScopeId": "grand"}),
                );
                nodes.insert("g0".into(), activity("g0", "grand"));
                nodes.insert(
                    "g-final".into(),
                    json!({"id": "g-final", "label": "Grand final", "scopeId": "grand",
                           "kind": "final"}),
                );
                scopes.insert(
                    "grand".into(),
                    json!({"id": "grand", "label": "Grand", "kind": "compound",
                           "parentScopeId": "child", "initialNodeIds": ["g0"],
                           "entryActivities": [], "exitActivities": [],
                           "history": "none"}),
                );
                link(&mut transitions, &mut groups, "g0", "g-final");
                link(&mut transitions, &mut groups, &previous, "inner");
                previous = "inner".to_owned();
            }
            nodes.insert(
                "c-final".into(),
                json!({"id": "c-final", "label": "Child final", "scopeId": "child",
                       "kind": "final"}),
            );
            link(&mut transitions, &mut groups, &previous, "c-final");
            scopes.insert(
                "child".into(),
                json!({"id": "child", "label": "Child", "kind": "compound",
                       "parentScopeId": "root", "initialNodeIds": ["c0"],
                       "entryActivities": entry_activities,
                       "exitActivities": exit_activities,
                       "history": "none"}),
            );
        }
        ScopeShape::Parallel { regions } => {
            let mut initial = Vec::with_capacity(regions);
            for region in 0..regions {
                let start = format!("r{region}");
                let done = format!("r{region}-final");
                nodes.insert(start.clone(), activity(&start, "child"));
                nodes.insert(
                    done.clone(),
                    json!({"id": done, "label": format!("Region {region} final"),
                           "scopeId": "child", "kind": "final"}),
                );
                link(&mut transitions, &mut groups, &start, &done);
                initial.push(start);
            }
            scopes.insert(
                "child".into(),
                json!({"id": "child", "label": "Child", "kind": "parallel",
                       "parentScopeId": "root", "initialNodeIds": initial,
                       "entryActivities": entry_activities,
                       "exitActivities": exit_activities,
                       "history": "none"}),
            );
        }
    }

    document(
        Value::Object(nodes),
        Value::Object(groups),
        Value::Object(transitions),
        Value::Object(scopes),
    )
}

pub(crate) fn arb_scoped_workflow() -> impl Strategy<Value = ScopedWorkflow> {
    (
        prop_oneof![
            ((1_usize..=3), any::<bool>())
                .prop_map(|(chain_length, nested)| ScopeShape::Compound {
                    chain_length,
                    nested,
                }),
            (2_usize..=3).prop_map(|regions| ScopeShape::Parallel { regions }),
        ],
        any::<bool>(),
    )
        .prop_map(|(shape, lifecycle)| ScopedWorkflow {
            workflow: parse_publishable(scoped_document(shape, lifecycle)),
            shape,
            lifecycle,
        })
}

// -------------------------------------------------------
// Command scripts and bounded JSON
// -------------------------------------------------------

/// Raw decision material the kernel driver interprets against the live run
/// state: each juncture (which pending effect to observe, whether to approve
/// or reject, which guard verdict to publish) consumes one byte modulo the
/// number of alternatives, so any byte vector is a valid script and shrinking
/// bytes toward zero shrinks toward the canonical first choice.
#[derive(Debug, Clone)]
pub(crate) struct DriverScript {
    pub choices: Vec<u8>,
    /// Inject a `Cancel` command at this driver iteration, if any.
    pub cancel_at: Option<usize>,
}

pub(crate) fn arb_command_sequence(
    _workflow: &WorkflowDefinition,
) -> impl Strategy<Value = DriverScript> + use<> {
    arb_driver_script()
}

pub(crate) fn arb_driver_script() -> impl Strategy<Value = DriverScript> {
    (
        proptest::collection::vec(any::<u8>(), 0..24),
        proptest::option::weighted(0.2, 0_usize..8),
    )
        .prop_map(|(choices, cancel_at)| DriverScript { choices, cancel_at })
}

/// Bounded JSON (depth ≤ 4) for effect outputs and orchestration payloads.
pub(crate) fn arb_json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-1_000_i64..1_000).prop_map(|number| json!(number)),
        "[a-z]{0,8}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            proptest::collection::btree_map("[a-z]{1,6}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}
