use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use anyhow::{Context, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A validated, deterministic Workflow Definition v2 document.
///
/// The canonical JSON is the persisted representation and the hash/signature
/// input.  The parsed form is retained so compilation never has to interpret
/// unvalidated JSON.  Object keys are sorted recursively before serialization,
/// making semantically identical documents stable across clients.
#[derive(Debug, Clone)]
pub struct WorkflowDefinition {
    canonical_json: String,
    parsed: Definition,
}

impl PartialEq for WorkflowDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_json == other.canonical_json
    }
}

impl Eq for WorkflowDefinition {}

impl Hash for WorkflowDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical_json.hash(state);
    }
}

impl WorkflowDefinition {
    pub fn new(value: Value) -> anyhow::Result<Self> {
        let parsed: Definition =
            serde_json::from_value(value).context("invalid Workflow Definition v2 document")?;
        parsed.validate_draft()?;
        let canonical = canonicalize(serde_json::to_value(&parsed)?);
        Ok(Self {
            canonical_json: serde_json::to_string(&canonical)?,
            parsed,
        })
    }

    pub fn from_json(json: impl AsRef<str>) -> anyhow::Result<Self> {
        Self::new(serde_json::from_str(json.as_ref()).context("workflow definition is not JSON")?)
    }

    pub fn json(self) -> String {
        self.canonical_json
    }

    pub fn json_ref(&self) -> &str {
        &self.canonical_json
    }

    pub const fn parsed(&self) -> &Definition {
        &self.parsed
    }

    pub fn definition_hash(&self) -> String {
        hex::encode(Sha256::digest(self.canonical_json.as_bytes()))
    }
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        scalar => scalar,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub schema_version: String,
    pub entry_scope_id: String,
    pub interface: WorkflowInterface,
    pub nodes: BTreeMap<String, WorkflowNode>,
    pub policies: WorkflowPolicies,
    pub presentation: WorkflowPresentation,
    pub route_groups: BTreeMap<String, RouteGroup>,
    pub scopes: BTreeMap<String, WorkflowScope>,
    pub transitions: BTreeMap<String, WorkflowTransition>,
    pub types: BTreeMap<String, PortableType>,
}

impl Definition {
    /// Protects serialization invariants while allowing incomplete drafts.
    pub fn validate_draft(&self) -> anyhow::Result<()> {
        ensure!(
            self.schema_version == "2.0",
            "unsupported workflow schema {}; expected 2.0",
            self.schema_version
        );
        validate_map_ids("node", &self.nodes, |node| &node.id)?;
        validate_map_ids("scope", &self.scopes, |scope| &scope.id)?;
        validate_map_ids("route group", &self.route_groups, |group| &group.id)?;
        validate_map_ids("transition", &self.transitions, |transition| &transition.id)?;
        Ok(())
    }

    /// Enforces all execution invariants at deploy/publish time.
    pub fn validate_publishable(&self) -> anyhow::Result<()> {
        self.validate_draft()?;
        ensure!(
            self.schema_version == "2.0",
            "unsupported workflow schema {}; expected 2.0",
            self.schema_version
        );
        ensure!(
            self.scopes.contains_key(&self.entry_scope_id),
            "entry scope {} does not exist",
            self.entry_scope_id
        );

        validate_map_ids("node", &self.nodes, |node| &node.id)?;
        validate_map_ids("scope", &self.scopes, |scope| &scope.id)?;
        validate_map_ids("route group", &self.route_groups, |group| &group.id)?;
        validate_map_ids("transition", &self.transitions, |transition| &transition.id)?;

        for node in self.nodes.values() {
            ensure!(
                self.scopes.contains_key(&node.scope_id),
                "node {} refers to missing scope {}",
                node.id,
                node.scope_id
            );
            if let NodeKind::Activity { activity } = &node.kind {
                validate_activity_reference(activity)
                    .with_context(|| format!("invalid activity on node {}", node.id))?;
            }
            if let Some(retry) = &node.policy.retry {
                retry
                    .validate()
                    .with_context(|| format!("invalid retry policy on node {}", node.id))?;
            }
            if let Some(timeout) = &node.policy.timeout {
                ensure!(
                    timeout.after_millis > 0,
                    "timeout on node {} must be positive",
                    node.id
                );
            }
            if let Some(max_output_bytes) = node.policy.max_output_bytes {
                ensure!(
                    max_output_bytes > 0,
                    "maxOutputBytes on node {} must be positive",
                    node.id
                );
            }
            validate_authorities(&node.policy.authorities)
                .with_context(|| format!("invalid authority policy on node {}", node.id))?;
            if let Some(compensation) = &node.policy.compensation {
                validate_activity_reference(compensation)
                    .with_context(|| format!("invalid compensation on node {}", node.id))?;
            }
            match &node.kind {
                NodeKind::Approval {
                    quorum, approvers, ..
                } => {
                    ensure!(*quorum > 0, "approval quorum on node {} is zero", node.id);
                    validate_authorities(approvers)
                        .with_context(|| format!("invalid approvers on node {}", node.id))?;
                    ensure!(
                        (!approvers.is_empty() && *quorum <= approvers.len())
                            || (approvers.is_empty() && *quorum == 1),
                        "approval quorum {} on node {} exceeds its approver set",
                        quorum,
                        node.id
                    );
                    if node.policy.timeout.is_some() {
                        ensure!(
                            self.transitions.values().any(|transition| {
                                transition.source_node_id == node.id
                                    && transition.trigger == "timedOut"
                            }),
                            "approval node {} declares a timeout but no timedOut route",
                            node.id
                        );
                    }
                }
                NodeKind::Wait {
                    wait_for,
                    signal,
                    timer,
                } => match wait_for {
                    WaitKind::Timer => {
                        let timer = timer.as_ref().with_context(|| {
                            format!("timer wait node {} has no timer policy", node.id)
                        })?;
                        ensure!(
                            timer.delay_millis.is_some() ^ timer.deadline.is_some(),
                            "timer wait node {} must define exactly one of delayMillis or deadline",
                            node.id
                        );
                        if let Some(delay) = timer.delay_millis {
                            ensure!(delay > 0, "timer wait node {} has zero delay", node.id);
                        }
                        ensure!(
                            signal.is_none(),
                            "timer wait node {} cannot also declare a signal",
                            node.id
                        );
                    }
                    WaitKind::Signal | WaitKind::Event => {
                        ensure!(
                            timer.is_none(),
                            "non-timer wait node {} cannot declare a timer",
                            node.id
                        );
                        if let Some(signal) = signal {
                            ensure!(
                                !signal.trim().is_empty(),
                                "wait node {} has an empty signal name",
                                node.id
                            );
                        }
                    }
                },
                _ => {}
            }
            if let Some(join) = &node.join {
                join.validate()
                    .with_context(|| format!("invalid join on node {}", node.id))?;
                if let JoinPolicy::Quorum { quorum, .. } = join {
                    let incoming = self
                        .transitions
                        .values()
                        .filter(|transition| transition.target_node_id == node.id)
                        .count();
                    ensure!(
                        *quorum <= incoming,
                        "quorum {} on node {} exceeds its {} incoming transitions",
                        quorum,
                        node.id,
                        incoming
                    );
                }
            }
        }

        for scope in self.scopes.values() {
            ensure!(
                (scope.id == self.entry_scope_id) == matches!(scope.kind, ScopeKind::Root),
                "only the entry scope may have kind root"
            );
            if let Some(parent) = &scope.parent_scope_id {
                ensure!(
                    self.scopes.contains_key(parent),
                    "scope {} refers to missing parent scope {}",
                    scope.id,
                    parent
                );
                ensure!(
                    parent != &scope.id,
                    "scope {} cannot parent itself",
                    scope.id
                );
            }
            for node_id in scope
                .initial_node_ids
                .iter()
                .chain(&scope.entry_activities)
                .chain(&scope.exit_activities)
            {
                let node = self.nodes.get(node_id).with_context(|| {
                    format!("scope {} refers to missing node {}", scope.id, node_id)
                })?;
                ensure!(
                    node.scope_id == scope.id,
                    "scope {} refers to node {} owned by scope {}",
                    scope.id,
                    node_id,
                    node.scope_id
                );
                if scope
                    .entry_activities
                    .iter()
                    .chain(&scope.exit_activities)
                    .any(|activity_id| activity_id == node_id)
                {
                    ensure!(
                        matches!(&node.kind, NodeKind::Activity { .. }),
                        "scope {} lifecycle node {} is not an activity",
                        scope.id,
                        node_id
                    );
                }
            }
            match scope.kind {
                ScopeKind::Parallel => ensure!(
                    !scope.initial_node_ids.is_empty(),
                    "parallel scope {} has no initial regions",
                    scope.id
                ),
                ScopeKind::Compound | ScopeKind::Root => ensure!(
                    scope.initial_node_ids.len() == 1,
                    "{} scope {} must have exactly one initial node",
                    match scope.kind {
                        ScopeKind::Root => "root",
                        _ => "compound",
                    },
                    scope.id
                ),
            }
        }

        self.validate_scope_hierarchy()?;

        let mut priorities = BTreeSet::new();
        let mut group_shapes = BTreeMap::<String, (String, String)>::new();
        for transition in self.transitions.values() {
            ensure!(
                self.nodes.contains_key(&transition.source_node_id),
                "transition {} refers to missing source node {}",
                transition.id,
                transition.source_node_id
            );
            ensure!(
                self.nodes.contains_key(&transition.target_node_id),
                "transition {} refers to missing target node {}",
                transition.id,
                transition.target_node_id
            );
            ensure!(
                self.route_groups.contains_key(&transition.route_group_id),
                "transition {} refers to missing route group {}",
                transition.id,
                transition.route_group_id
            );
            if let Some(loop_policy) = &transition.loop_policy {
                loop_policy.validate().with_context(|| {
                    format!("invalid loop policy on transition {}", transition.id)
                })?;
            }
            if let Some(guard) = &transition.guard {
                guard
                    .validate()
                    .with_context(|| format!("invalid guard on transition {}", transition.id))?;
            }
            validate_trigger(&transition.trigger)
                .with_context(|| format!("invalid trigger on transition {}", transition.id))?;
            if let Some(source_port) = &transition.source_port {
                ensure!(
                    !source_port.trim().is_empty(),
                    "transition {} has an empty source port",
                    transition.id
                );
            }
            if let Some(target_port) = &transition.target_port {
                ensure!(
                    !target_port.trim().is_empty(),
                    "transition {} has an empty target port",
                    transition.id
                );
            }
            let group = &self.route_groups[&transition.route_group_id];
            match group_shapes.get(&transition.route_group_id) {
                Some((source, trigger)) => ensure!(
                    source == &transition.source_node_id && trigger == &transition.trigger,
                    "route group {} mixes source nodes or trigger dispositions",
                    transition.route_group_id
                ),
                None => {
                    group_shapes.insert(
                        transition.route_group_id.clone(),
                        (
                            transition.source_node_id.clone(),
                            transition.trigger.clone(),
                        ),
                    );
                }
            }
            if group.mode == RoutingMode::FirstMatch {
                ensure!(
                    priorities.insert((transition.route_group_id.clone(), transition.priority)),
                    "first-match route group {} contains duplicate priority {}",
                    transition.route_group_id,
                    transition.priority
                );
            }
        }

        self.validate_cycles()
    }

    fn validate_scope_hierarchy(&self) -> anyhow::Result<()> {
        let roots = self
            .scopes
            .values()
            .filter(|scope| scope.parent_scope_id.is_none())
            .collect::<Vec<_>>();
        ensure!(
            roots.len() == 1,
            "workflow must contain exactly one root scope"
        );
        ensure!(
            roots[0].id == self.entry_scope_id && matches!(roots[0].kind, ScopeKind::Root),
            "entry scope must be the unique root scope"
        );

        for scope in self.scopes.values() {
            let mut seen = BTreeSet::new();
            let mut current = Some(scope.id.as_str());
            while let Some(scope_id) = current {
                ensure!(
                    seen.insert(scope_id.to_owned()),
                    "scope hierarchy contains a cycle through {scope_id}"
                );
                current = self.scopes[scope_id].parent_scope_id.as_deref();
            }
        }

        let mut owners = BTreeMap::<&str, Vec<&str>>::new();
        for node in self.nodes.values() {
            if let NodeKind::Scope { child_scope_id } = &node.kind {
                owners
                    .entry(child_scope_id.as_str())
                    .or_default()
                    .push(node.id.as_str());
                let child = self.scopes.get(child_scope_id).with_context(|| {
                    format!(
                        "scope node {} refers to missing scope {child_scope_id}",
                        node.id
                    )
                })?;
                ensure!(
                    child.parent_scope_id.as_deref() == Some(node.scope_id.as_str()),
                    "scope node {} enters {}, whose parent is not {}",
                    node.id,
                    child_scope_id,
                    node.scope_id
                );
            }
        }
        for scope in self.scopes.values() {
            let scope_owners = owners
                .get(scope.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            if scope.id == self.entry_scope_id {
                ensure!(
                    scope_owners.is_empty(),
                    "entry scope {} cannot be owned by a scope node",
                    scope.id
                );
            } else {
                ensure!(
                    scope_owners.len() == 1,
                    "non-root scope {} must have exactly one owning scope node; found {} ({})",
                    scope.id,
                    scope_owners.len(),
                    scope_owners.join(", ")
                );
            }
        }
        Ok(())
    }

    fn validate_cycles(&self) -> anyhow::Result<()> {
        struct Tarjan<'a> {
            adjacency: &'a BTreeMap<String, Vec<String>>,
            next_index: &'a mut usize,
            indexes: &'a mut BTreeMap<String, usize>,
            low_links: &'a mut BTreeMap<String, usize>,
            stack: &'a mut Vec<String>,
            on_stack: &'a mut BTreeSet<String>,
            components: &'a mut Vec<Vec<String>>,
        }
        impl Tarjan<'_> {
            fn visit(&mut self, node: &str) {
                let index = *self.next_index;
                *self.next_index += 1;
                self.indexes.insert(node.to_owned(), index);
                self.low_links.insert(node.to_owned(), index);
                self.stack.push(node.to_owned());
                self.on_stack.insert(node.to_owned());

                for target in self.adjacency.get(node).into_iter().flatten() {
                    if !self.indexes.contains_key(target) {
                        self.visit(target);
                        let low = self.low_links[node].min(self.low_links[target]);
                        self.low_links.insert(node.to_owned(), low);
                    } else if self.on_stack.contains(target) {
                        let low = self.low_links[node].min(self.indexes[target]);
                        self.low_links.insert(node.to_owned(), low);
                    }
                }

                if self.low_links[node] == self.indexes[node] {
                    let mut component = Vec::new();
                    while let Some(member) = self.stack.pop() {
                        self.on_stack.remove(&member);
                        let done = member == node;
                        component.push(member);
                        if done {
                            break;
                        }
                    }
                    self.components.push(component);
                }
            }
        }

        let mut next_index = 0usize;
        let mut indexes = BTreeMap::<String, usize>::new();
        let mut low_links = BTreeMap::<String, usize>::new();
        let mut stack = Vec::<String>::new();
        let mut on_stack = BTreeSet::<String>::new();
        let mut components = Vec::<Vec<String>>::new();
        // Remove every progress boundary. The remaining graph must be acyclic;
        // otherwise some smaller cycle can bypass a bound/wait even if its
        // enclosing SCC contains one.
        let hot_nodes = self
            .nodes
            .iter()
            .filter(|(_, node)| !matches!(node.kind, NodeKind::Wait { .. }))
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        let mut adjacency = hot_nodes
            .iter()
            .map(|id| (id.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for transition in self.transitions.values().filter(|transition| {
            transition.loop_policy.is_none()
                && hot_nodes.contains(&transition.source_node_id)
                && hot_nodes.contains(&transition.target_node_id)
        }) {
            adjacency
                .entry(transition.source_node_id.clone())
                .or_default()
                .push(transition.target_node_id.clone());
        }

        {
            let mut tarjan = Tarjan {
                adjacency: &adjacency,
                next_index: &mut next_index,
                indexes: &mut indexes,
                low_links: &mut low_links,
                stack: &mut stack,
                on_stack: &mut on_stack,
                components: &mut components,
            };
            for node in adjacency.keys() {
                if !tarjan.indexes.contains_key(node) {
                    tarjan.visit(node);
                }
            }
        }

        for component in components {
            let members = component.iter().cloned().collect::<BTreeSet<_>>();
            let internal = self
                .transitions
                .values()
                .filter(|transition| {
                    transition.loop_policy.is_none()
                        && members.contains(&transition.source_node_id)
                        && members.contains(&transition.target_node_id)
                })
                .collect::<Vec<_>>();
            let cyclic = component.len() > 1
                || internal
                    .iter()
                    .any(|transition| transition.source_node_id == transition.target_node_id);
            if !cyclic {
                continue;
            }
            bail!(
                "cycle [{}] can avoid every bounded transition, durable-yield transition, and wait node",
                component.join(", ")
            );
        }
        Ok(())
    }

    pub fn agent_references(&self) -> BTreeMap<(String, String), String> {
        self.nodes
            .values()
            .filter_map(|node| match &node.kind {
                NodeKind::Activity {
                    activity:
                        ActivityReference::Agent {
                            agent_id,
                            agent_version,
                            ..
                        },
                } => Some((
                    (agent_id.clone(), agent_version.clone()),
                    node.label.clone(),
                )),
                _ => None,
            })
            .collect()
    }
}

fn validate_map_ids<T>(
    kind: &str,
    values: &BTreeMap<String, T>,
    id: impl Fn(&T) -> &String,
) -> anyhow::Result<()> {
    for (key, value) in values {
        ensure!(
            key == id(value),
            "{kind} map key {key} differs from embedded id {}",
            id(value)
        );
    }
    Ok(())
}

fn validate_authorities(authorities: &[String]) -> anyhow::Result<()> {
    let mut unique = BTreeSet::new();
    for authority in authorities {
        ensure!(!authority.trim().is_empty(), "authority cannot be empty");
        ensure!(
            unique.insert(authority),
            "authority {authority:?} is declared more than once"
        );
    }
    Ok(())
}

fn validate_trigger(trigger: &str) -> anyhow::Result<()> {
    const STANDARD: [&str; 9] = [
        "cancelled",
        "completed",
        "compensated",
        "failed",
        "iterationLimit",
        "rejected",
        "resourceExhausted",
        "skipped",
        "timedOut",
    ];
    if STANDARD.contains(&trigger) {
        return Ok(());
    }
    let name = trigger
        .strip_prefix("custom:")
        .context("trigger must be a standard or custom disposition")?;
    ensure!(
        (1..=128).contains(&name.len())
            && name
                .chars()
                .enumerate()
                .all(|(index, character)| character.is_ascii_alphanumeric()
                    && (index > 0 || character.is_ascii_alphabetic())
                    || (index > 0 && matches!(character, '.' | '_' | '-'))),
        "custom disposition must start with an ASCII letter and contain at most 128 ASCII letters, digits, dots, underscores, or hyphens"
    );
    Ok(())
}

fn validate_activity_reference(activity: &ActivityReference) -> anyhow::Result<()> {
    match activity {
        ActivityReference::Agent {
            agent_id,
            agent_version,
            ..
        } => {
            ensure!(!agent_id.trim().is_empty(), "agent id is empty");
            ensure!(!agent_version.trim().is_empty(), "agent version is empty");
        }
        ActivityReference::Builtin { .. } => {}
        ActivityReference::ForeignTerm {
            abi,
            artifact_hash,
            source,
            language_tag,
            language_specification_hash,
            resources,
            authorities,
        } => validate_foreign_term(
            abi,
            artifact_hash,
            source,
            language_tag,
            language_specification_hash,
            resources,
            authorities,
        )?,
        ActivityReference::Rholang { uri, .. } => {
            ensure!(
                uri.starts_with("rho:id:") && uri.len() > "rho:id:".len(),
                "Rholang activity must name a rho:id: registry URI"
            );
        }
        ActivityReference::Workflow { team_id, version } => {
            ensure!(
                team_id.starts_with("rho:id:") && team_id.len() > "rho:id:".len(),
                "nested workflow must name a rho:id: team URI"
            );
            ensure!(
                !version.trim().is_empty(),
                "nested workflow version is empty"
            );
        }
    }
    Ok(())
}

fn validate_foreign_term(
    abi: &ForeignTermAbi,
    artifact_hash: &str,
    source: &str,
    language_tag: &str,
    language_specification_hash: &str,
    resources: &ForeignTermResources,
    authorities: &[String],
) -> anyhow::Result<()> {
    ensure!(
        abi.version == "flt-v1",
        "unsupported FLT ABI {}",
        abi.version
    );
    ensure!(!abi.entrypoint.trim().is_empty(), "FLT entrypoint is empty");
    ensure!(
        !abi.input_binding.trim().is_empty() && !abi.output_binding.trim().is_empty(),
        "FLT input/output bindings cannot be empty"
    );
    ensure!(
        !source.is_empty() && source.len() <= 1_048_576,
        "FLT source must contain between 1 and 1048576 bytes"
    );
    ensure!(
        !language_tag.is_empty()
            && language_tag
                .chars()
                .enumerate()
                .all(|(index, character)| character.is_ascii_alphanumeric()
                    || (index > 0 && (character == '_' || character == '-'))),
        "FLT language tag is not a safe registered-language identifier"
    );
    ensure!(
        !language_specification_hash.trim().is_empty(),
        "FLT language specification hash is empty"
    );
    ensure!(
        artifact_hash == hex::encode(Sha256::digest(source.as_bytes())),
        "FLT artifact hash does not match its source"
    );
    ensure!(
        !source.contains('`'),
        "FLT source cannot contain the Rholang FLT delimiter"
    );
    ensure!(
        resources.fuel > 0 && resources.max_output_bytes > 0 && resources.max_phlo > 0,
        "FLT resource limits must be positive"
    );
    validate_authorities(authorities)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowInterface {
    pub inputs: BTreeMap<String, PortableType>,
    pub outputs: BTreeMap<String, PortableType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PortableType {
    Artifact,
    Bool,
    Bytes,
    Capability,
    Duration,
    Error,
    Int,
    Legacy,
    String,
    Timestamp,
    Unit,
    List { value: Box<Self> },
    Record { fields: BTreeMap<String, Self> },
    Variant { variants: BTreeMap<String, Self> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowPolicies {
    pub default_execution: ExecutionMode,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Auto,
    Bounded,
    Durable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowPresentation {
    pub edges: BTreeMap<String, EdgePresentation>,
    pub nodes: BTreeMap<String, NodePresentation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgePresentation {
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodePresentation {
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNode {
    pub id: String,
    pub label: String,
    pub scope_id: String,
    pub join: Option<JoinPolicy>,
    #[serde(default)]
    pub policy: NodeExecutionPolicy,
    #[serde(flatten)]
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeExecutionPolicy {
    pub retry: Option<RetryPolicy>,
    pub timeout: Option<TimeoutPolicy>,
    pub compensation: Option<ActivityReference>,
    #[serde(default)]
    pub authorities: Vec<String>,
    pub max_output_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay_millis: u64,
    pub backoff_multiplier_millis: u32,
    pub max_delay_millis: u64,
    #[serde(default)]
    pub deterministic_jitter: bool,
    #[serde(default)]
    pub retry_on: Vec<String>,
}

impl RetryPolicy {
    fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.max_attempts > 0, "retry maxAttempts must be positive");
        ensure!(
            self.backoff_multiplier_millis >= 1000,
            "retry backoffMultiplierMillis must be at least 1000"
        );
        ensure!(
            self.initial_delay_millis <= self.max_delay_millis,
            "retry initialDelayMillis exceeds maxDelayMillis"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeoutPolicy {
    pub after_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NodeKind {
    Approval {
        policy_id: String,
        #[serde(default = "default_approval_quorum")]
        quorum: usize,
        #[serde(default)]
        approvers: Vec<String>,
    },
    Activity {
        activity: ActivityReference,
    },
    Final,
    Gateway,
    History {
        history: HistoryMode,
    },
    Input,
    Output,
    Scope {
        child_scope_id: String,
    },
    Transform {
        transform: TransformKind,
    },
    Wait {
        wait_for: WaitKind,
        #[serde(default)]
        signal: Option<String>,
        #[serde(default)]
        timer: Option<TimerPolicy>,
    },
}

const fn default_approval_quorum() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimerPolicy {
    pub delay_millis: Option<u64>,
    pub deadline: Option<ValueExpression>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivityReference {
    Agent {
        agent_id: String,
        agent_version: String,
        protocol: AgentProtocol,
    },
    Builtin {
        name: BuiltinActivity,
    },
    ForeignTerm {
        abi: ForeignTermAbi,
        artifact_hash: String,
        source: String,
        language_tag: String,
        language_specification_hash: String,
        resources: ForeignTermResources,
        #[serde(default)]
        authorities: Vec<String>,
    },
    Rholang {
        protocol: AgentProtocol,
        uri: String,
    },
    Workflow {
        team_id: String,
        version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForeignTermAbi {
    pub version: String,
    pub entrypoint: String,
    pub input_binding: String,
    pub output_binding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForeignTermResources {
    pub fuel: u64,
    pub max_output_bytes: u64,
    pub max_phlo: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentProtocol {
    #[serde(rename = "simple-v1")]
    SimpleV1,
    #[serde(rename = "structured-v1")]
    StructuredV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinActivity {
    Compress,
    TextModel,
    TtiModel,
    TtsModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryMode {
    Deep,
    None,
    Shallow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransformKind {
    FanInMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WaitKind {
    Event,
    Signal,
    Timer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum JoinPolicy {
    All {
        correlation: CorrelationPolicy,
    },
    Each {
        correlation: CorrelationPolicy,
    },
    Quorum {
        correlation: CorrelationPolicy,
        quorum: usize,
        late: LateArrival,
    },
    Race {
        correlation: CorrelationPolicy,
        late: LateArrival,
    },
    Reduce {
        correlation: CorrelationPolicy,
        reducer: ReduceOperation,
        until: Condition,
    },
}

impl JoinPolicy {
    fn validate(&self) -> anyhow::Result<()> {
        match self {
            Self::Quorum { quorum, .. } => ensure!(*quorum > 0, "quorum must be positive"),
            Self::Reduce { until, .. } => {
                until.validate()?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReduceOperation {
    Collect,
    First,
    Last,
    Max,
    MergeRecords,
    Min,
    Sum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CorrelationPolicy {
    Key { value: ValueExpression },
    Root,
    RouteGroup { route_group_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LateArrival {
    Cancel,
    Discard,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTransition {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub trigger: String,
    pub guard: Option<Condition>,
    pub priority: i64,
    pub route_group_id: String,
    #[serde(rename = "loop")]
    pub loop_policy: Option<LoopPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum LoopPolicy {
    Bounded {
        #[serde(rename = "maxIterations")]
        max_iterations: u64,
    },
    Durable {
        #[serde(rename = "durableYield")]
        durable_yield: bool,
    },
}

impl LoopPolicy {
    fn validate(&self) -> anyhow::Result<()> {
        match self {
            Self::Bounded { max_iterations } => {
                ensure!(*max_iterations > 0, "maxIterations must be positive");
            }
            Self::Durable { durable_yield } => ensure!(*durable_yield, "durableYield must be true"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteGroup {
    pub id: String,
    pub mode: RoutingMode,
    pub no_match: NoMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingMode {
    AllMatches,
    FirstMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoMatch {
    Complete,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowScope {
    pub id: String,
    pub label: String,
    pub kind: ScopeKind,
    pub parent_scope_id: Option<String>,
    pub initial_node_ids: Vec<String>,
    pub entry_activities: Vec<String>,
    pub exit_activities: Vec<String>,
    pub history: HistoryMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScopeKind {
    Compound,
    Parallel,
    Root,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Condition {
    All {
        operands: Vec<Self>,
    },
    Any {
        operands: Vec<Self>,
    },
    Compare {
        left: ValueExpression,
        operator: Comparison,
        right: ValueExpression,
    },
    Exists {
        expression: ValueExpression,
    },
    Literal {
        value: bool,
    },
    Not {
        operand: Box<Self>,
    },
    Rholang {
        inputs: BTreeMap<String, ValuePath>,
        source: String,
        source_hash: Option<String>,
    },
}

impl Condition {
    fn validate(&self) -> anyhow::Result<()> {
        match self {
            Self::All { operands } | Self::Any { operands } => {
                ensure!(
                    !operands.is_empty(),
                    "boolean condition must have at least one operand"
                );
                for operand in operands {
                    operand.validate()?;
                }
            }
            Self::Not { operand } => operand.validate()?,
            Self::Rholang {
                inputs,
                source,
                source_hash,
            } => {
                validate_pure_rholang_guard(inputs, source)?;
                if let Some(source_hash) = source_hash {
                    ensure!(
                        source_hash == &hex::encode(Sha256::digest(source.as_bytes())),
                        "Rholang guard sourceHash does not match its source"
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn validate_pure_rholang_guard(
    inputs: &BTreeMap<String, ValuePath>,
    source: &str,
) -> anyhow::Result<()> {
    let source = source.trim();
    ensure!(!source.is_empty(), "Rholang guard cannot be empty");
    ensure!(source.len() <= 4096, "Rholang guard exceeds 4096 bytes");
    ensure!(
        !source.contains("!(")
            && !source.contains("!!")
            && !source.contains("?(")
            && !source.contains('`'),
        "Rholang guard contains a send, receive, or URI literal"
    );
    let tokens = source
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect::<BTreeSet<_>>();
    for forbidden in [
        "new", "for", "contract", "bundle", "match", "stdout", "registry",
    ] {
        if tokens.contains(forbidden) {
            bail!("Rholang guard contains effectful or binding form {forbidden:?}");
        }
    }
    for name in inputs.keys() {
        ensure!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "guard input name {name:?} is not a safe identifier"
        );
        ensure!(
            source.contains(&format!("${name}")),
            "guard input {name:?} is declared but unused"
        );
    }
    let mut references = BTreeSet::new();
    let bytes = source.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() {
        if bytes[offset] != b'$' {
            offset += 1;
            continue;
        }
        let start = offset + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        ensure!(
            end > start,
            "every $ in a Rholang guard must introduce an input"
        );
        let name = &source[start..end];
        ensure!(
            inputs.contains_key(name),
            "guard input ${name} is not declared"
        );
        references.insert(name);
        offset = end;
    }
    ensure!(
        references.len() == inputs.len(),
        "every declared Rholang guard input must be referenced"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Comparison {
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
    Ne,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ValueExpression {
    Literal {
        value: Value,
    },
    Path {
        #[serde(flatten)]
        value: ValuePath,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValuePath {
    pub root: ValueRoot,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueRoot {
    Error,
    Input,
    Output,
    State,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::WorkflowDefinition;

    fn definition(edge: serde_json::Value) -> serde_json::Value {
        let mut definition = json!({
            "schemaVersion": "2.0",
            "entryScopeId": "root",
            "interface": { "inputs": {"input": {"kind": "string"}}, "outputs": {"output": {"kind": "legacy"}} },
            "nodes": {
                "in": {"id":"in", "label":"Input", "scopeId":"root", "kind":"input"},
                "work": {"id":"work", "label":"Work", "scopeId":"root", "kind":"gateway"}
            },
            "policies": {"defaultExecution":"auto", "visibility":"private"},
            "presentation": {"edges": {}, "nodes": {}},
            "routeGroups": {"route": {"id":"route", "mode":"allMatches", "noMatch":"fail"}},
            "scopes": {"root": {"id":"root", "label":"Root", "kind":"root", "initialNodeIds":["in"], "entryActivities":[], "exitActivities":[], "history":"none"}},
            "transitions": {},
            "types": {}
        });
        definition["transitions"]["edge"] = edge;
        definition
    }

    #[test]
    fn rejects_hot_cycle_and_accepts_bound() {
        let hot = json!({"id":"edge", "sourceNodeId":"work", "targetNodeId":"work", "trigger":"completed", "priority":0, "routeGroupId":"route"});
        let hot = WorkflowDefinition::new(definition(hot)).unwrap();
        assert!(
            hot.parsed()
                .validate_publishable()
                .unwrap_err()
                .to_string()
                .contains("can avoid")
        );
        let bounded = json!({"id":"edge", "sourceNodeId":"work", "targetNodeId":"work", "trigger":"completed", "priority":0, "routeGroupId":"route", "loop":{"maxIterations":3}});
        WorkflowDefinition::new(definition(bounded))
            .expect("bounded loop is a valid draft")
            .parsed()
            .validate_publishable()
            .expect("bounded loop is publishable");
    }

    #[test]
    fn canonicalization_is_independent_of_object_key_order() {
        let a = WorkflowDefinition::new(definition(json!({"id":"edge", "sourceNodeId":"in", "targetNodeId":"work", "trigger":"completed", "priority":0, "routeGroupId":"route"}))).unwrap();
        let reparsed = WorkflowDefinition::from_json(a.json_ref()).unwrap();
        assert_eq!(a, reparsed);
    }

    #[test]
    fn pure_guard_rejects_undeclared_inputs_and_effects() {
        let undeclared = json!({
            "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
            "trigger":"completed", "priority":0, "routeGroupId":"route",
            "guard":{"kind":"rholang", "inputs":{}, "source":"$missing == true"}
        });
        let undeclared_definition = WorkflowDefinition::new(definition(undeclared)).unwrap();
        let error = undeclared_definition
            .parsed()
            .validate_publishable()
            .unwrap_err();
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("not declared")),
            "unexpected validation error: {error:#}"
        );

        let effectful = json!({
            "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
            "trigger":"completed", "priority":0, "routeGroupId":"route",
            "guard":{"kind":"rholang", "inputs":{}, "source":"new x in { true }"}
        });
        let effectful_definition = WorkflowDefinition::new(definition(effectful)).unwrap();
        assert!(
            effectful_definition
                .parsed()
                .validate_publishable()
                .is_err()
        );
    }

    #[test]
    fn rholang_guard_source_hash_is_verified() {
        let source = "$output == true";
        let guarded = |source_hash: String| {
            definition(json!({
                "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
                "trigger":"completed", "priority":0, "routeGroupId":"route",
                "guard":{
                    "kind":"rholang",
                    "inputs":{"output":{"root":"output","path":[]}},
                    "source":source,
                    "sourceHash":source_hash
                }
            }))
        };

        let mismatch = WorkflowDefinition::new(guarded("0".repeat(64))).unwrap();
        assert!(
            mismatch
                .parsed()
                .validate_publishable()
                .expect_err("a mismatched source hash must fail publication")
                .chain()
                .any(|cause| cause.to_string().contains("does not match"))
        );

        let expected = hex::encode(Sha256::digest(source.as_bytes()));
        WorkflowDefinition::new(guarded(expected))
            .unwrap()
            .parsed()
            .validate_publishable()
            .expect("an exact source hash must be publishable");
    }

    #[test]
    fn custom_dispositions_use_bounded_safe_names() {
        for invalid in [
            "custom:",
            "custom:9starts-with-a-number",
            "custom:contains space",
            "unknown",
        ] {
            let workflow = WorkflowDefinition::new(definition(json!({
                "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
                "trigger":invalid, "priority":0, "routeGroupId":"route"
            })))
            .unwrap();
            assert!(
                workflow.parsed().validate_publishable().is_err(),
                "trigger {invalid:?} should be rejected"
            );
        }

        WorkflowDefinition::new(definition(json!({
            "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
            "trigger":"custom:review.ready_v2", "priority":0, "routeGroupId":"route"
        })))
        .unwrap()
        .parsed()
        .validate_publishable()
        .expect("a safe custom disposition must be publishable");
    }

    #[test]
    fn every_non_root_scope_has_exactly_one_owner() {
        let scoped = |owners: usize| {
            let mut value = definition(json!({
                "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
                "trigger":"completed", "priority":0, "routeGroupId":"route"
            }));
            value["scopes"]["child"] = json!({
                "id":"child","label":"Child","kind":"compound","parentScopeId":"root",
                "initialNodeIds":["child-in"],"entryActivities":[],"exitActivities":[],"history":"none"
            });
            value["nodes"]["child-in"] =
                json!({"id":"child-in","label":"Child input","scopeId":"child","kind":"input"});
            for index in 0..owners {
                let id = format!("owner-{index}");
                value["nodes"][&id] = json!({
                    "id":id,"label":"Owner","scopeId":"root","kind":"scope","childScopeId":"child"
                });
            }
            WorkflowDefinition::new(value).expect("scoped definition should parse")
        };

        let missing = scoped(0);
        assert!(
            missing
                .parsed()
                .validate_publishable()
                .expect_err("unowned scope must fail")
                .to_string()
                .contains("exactly one owning scope node")
        );
        let duplicate = scoped(2);
        assert!(
            duplicate
                .parsed()
                .validate_publishable()
                .expect_err("multiply-owned scope must fail")
                .to_string()
                .contains("found 2")
        );
        scoped(1)
            .parsed()
            .validate_publishable()
            .expect("exactly one scope owner is valid");
    }

    #[test]
    fn compensation_activities_are_validated_like_forward_activities() {
        let mut value = definition(json!({
            "id":"edge", "sourceNodeId":"in", "targetNodeId":"work",
            "trigger":"completed", "priority":0, "routeGroupId":"route"
        }));
        value["nodes"]["work"] = json!({
            "id":"work","label":"Work","scopeId":"root","kind":"activity",
            "activity":{"kind":"builtin","name":"compress"},
            "policy":{
                "authorities":[],
                "compensation":{"kind":"rholang","protocol":"structured-v1","uri":"not-a-registry-uri"}
            }
        });
        let workflow =
            WorkflowDefinition::new(value).expect("invalid compensation is still a valid draft");
        let error = workflow
            .parsed()
            .validate_publishable()
            .expect_err("invalid compensation URI must fail publication");
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("rho:id:")),
            "unexpected validation error: {error:#}"
        );
    }

    #[test]
    fn approvals_with_timeouts_must_route_timed_out() {
        let approval_document = |gate_policy: serde_json::Value,
                                 transitions: serde_json::Value,
                                 groups: serde_json::Value| {
            json!({
                "schemaVersion": "2.0",
                "entryScopeId": "root",
                "interface": { "inputs": {"input": {"kind": "string"}}, "outputs": {"output": {"kind": "legacy"}} },
                "nodes": {
                    "in": {"id":"in", "label":"Input", "scopeId":"root", "kind":"input"},
                    "gate": {"id":"gate", "label":"Gate", "scopeId":"root", "kind":"approval",
                             "policyId":"owner", "quorum":1, "approvers":["alice"], "policy": gate_policy},
                    "out": {"id":"out", "label":"Output", "scopeId":"root", "kind":"output"},
                    "late": {"id":"late", "label":"Late", "scopeId":"root", "kind":"output"}
                },
                "policies": {"defaultExecution":"auto", "visibility":"private"},
                "presentation": {"edges": {}, "nodes": {}},
                "routeGroups": groups,
                "scopes": {"root": {"id":"root", "label":"Root", "kind":"root", "initialNodeIds":["in"], "entryActivities":[], "exitActivities":[], "history":"none"}},
                "transitions": transitions,
                "types": {}
            })
        };
        let base_groups = json!({
            "start": {"id":"start", "mode":"allMatches", "noMatch":"fail"},
            "ok": {"id":"ok", "mode":"allMatches", "noMatch":"fail"}
        });
        let base_transitions = json!({
            "start": {"id":"start", "sourceNodeId":"in", "targetNodeId":"gate", "trigger":"completed", "priority":0, "routeGroupId":"start"},
            "ok": {"id":"ok", "sourceNodeId":"gate", "targetNodeId":"out", "trigger":"completed", "priority":0, "routeGroupId":"ok"}
        });

        let unrouted = WorkflowDefinition::new(approval_document(
            json!({"timeout": {"afterMillis": 1000}}),
            base_transitions.clone(),
            base_groups.clone(),
        ))
        .expect("timeout without a timedOut route is still a valid draft");
        assert!(
            unrouted
                .parsed()
                .validate_publishable()
                .expect_err("a declared timeout must route timedOut")
                .to_string()
                .contains("declares a timeout but no timedOut route")
        );

        let mut routed_transitions = base_transitions.clone();
        routed_transitions["late"] = json!({"id":"late", "sourceNodeId":"gate", "targetNodeId":"late", "trigger":"timedOut", "priority":0, "routeGroupId":"late"});
        let mut routed_groups = base_groups.clone();
        routed_groups["late"] = json!({"id":"late", "mode":"allMatches", "noMatch":"fail"});
        WorkflowDefinition::new(approval_document(
            json!({"timeout": {"afterMillis": 1000}}),
            routed_transitions,
            routed_groups,
        ))
        .expect("routed timeout is a valid draft")
        .parsed()
        .validate_publishable()
        .expect("a timeout with a timedOut route is publishable");

        WorkflowDefinition::new(approval_document(json!({}), base_transitions, base_groups))
            .expect("timeout-free approval is a valid draft")
            .parsed()
            .validate_publishable()
            .expect("an approval without a timeout needs no timedOut route");
    }
}
