use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Context, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::workflow::{
    ActivityReference,
    Comparison,
    Condition,
    CorrelationPolicy,
    Definition,
    HistoryMode,
    JoinPolicy,
    LateArrival,
    LoopPolicy,
    NoMatch,
    NodeKind,
    ReduceOperation,
    RoutingMode,
    ScopeKind,
    ValueExpression,
    ValuePath,
    ValueRoot,
    WaitKind,
    WorkflowNode,
    WorkflowTransition,
};

pub const CONTINUATION_SCHEMA_VERSION: &str = "embers-agent-team-continuation-v1";

fn default_execution_id() -> String {
    "root".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContinuationEnvelope {
    pub schema_version: String,
    pub run_id: String,
    #[serde(default = "default_execution_id")]
    pub execution_id: String,
    pub team_uri: String,
    pub definition_hash: String,
    pub revision: u64,
    pub original_input: Value,
    pub state: Value,
    pub queue: VecDeque<Token>,
    pub joins: BTreeMap<String, JoinState>,
    pub scopes: BTreeMap<String, ScopeInstance>,
    pub history: BTreeMap<String, ScopeHistory>,
    pub compensations: Vec<CompensationRecord>,
    pub pending_effects: BTreeMap<String, PendingEffect>,
    pub pending_waits: BTreeMap<String, PendingWait>,
    #[serde(default)]
    pub nested_workflows: BTreeMap<String, Box<Self>>,
    pub guard_results: BTreeMap<String, bool>,
    pub outputs: Vec<SequencedValue>,
    pub events: Vec<KernelEvent>,
    pub next_token_sequence: u64,
    pub next_scope_sequence: u64,
    pub next_event_sequence: u64,
    pub cancelling: bool,
}

impl ContinuationEnvelope {
    pub fn start(
        definition: &Definition,
        run_id: String,
        team_uri: String,
        definition_hash: String,
        input: Value,
    ) -> anyhow::Result<Self> {
        let entry = definition
            .scopes
            .get(&definition.entry_scope_id)
            .context("entry scope disappeared after validation")?;
        let mut continuation = Self {
            schema_version: CONTINUATION_SCHEMA_VERSION.to_owned(),
            run_id,
            execution_id: default_execution_id(),
            team_uri,
            definition_hash,
            revision: 0,
            original_input: input,
            state: Value::Object(Map::new()),
            queue: VecDeque::with_capacity(entry.initial_node_ids.len()),
            joins: BTreeMap::new(),
            scopes: BTreeMap::new(),
            history: BTreeMap::new(),
            compensations: Vec::new(),
            pending_effects: BTreeMap::new(),
            pending_waits: BTreeMap::new(),
            nested_workflows: BTreeMap::new(),
            guard_results: BTreeMap::new(),
            outputs: Vec::new(),
            events: Vec::new(),
            next_token_sequence: 0,
            next_scope_sequence: 0,
            next_event_sequence: 0,
            cancelling: false,
        };
        continuation.event("runCreated", None, None);
        continuation.event("runKernelStarted", None, None);
        for node_id in &entry.initial_node_ids {
            let token = continuation.new_token(
                node_id.clone(),
                continuation.original_input.clone(),
                "entry".into(),
            );
            continuation.queue.push_back(token);
        }
        Ok(continuation)
    }

    pub fn start_nested(
        definition: &Definition,
        run_id: String,
        execution_id: String,
        team_uri: String,
        definition_hash: String,
        input: Value,
    ) -> anyhow::Result<Self> {
        let mut continuation = Self::start(definition, run_id, team_uri, definition_hash, input)?;
        continuation.execution_id = execution_id;
        Ok(continuation)
    }

    fn new_token(&mut self, node_id: String, value: Value, source_label: String) -> Token {
        let sequence = self.next_token_sequence;
        self.next_token_sequence += 1;
        Token {
            id: format!("token:{sequence}"),
            node_id,
            value,
            disposition: "completed".to_owned(),
            source_label,
            source_transition_id: None,
            iterations: BTreeMap::new(),
            fork_generations: BTreeMap::new(),
            correlations: BTreeMap::new(),
            epoch_stack: Vec::new(),
            scope_stack: Vec::new(),
            attempt: 1,
            joined: false,
            phase: TokenPhase::Enter,
            purpose: TokenPurpose::Normal,
        }
    }

    fn clone_token(&mut self, token: &Token) -> Token {
        let sequence = self.next_token_sequence;
        self.next_token_sequence += 1;
        let mut cloned = token.clone();
        cloned.id = format!("token:{sequence}");
        cloned
    }

    fn event(&mut self, event_type: &str, node_id: Option<String>, detail_hash: Option<String>) {
        self.events.push(KernelEvent {
            sequence: self.next_event_sequence,
            event_type: event_type.to_owned(),
            node_id,
            detail_hash,
        });
        self.next_event_sequence += 1;
    }

    pub(crate) fn record_public_event(&mut self, event_type: &str) {
        if self
            .events
            .last()
            .is_none_or(|event| event.event_type != event_type)
        {
            self.event(event_type, None, None);
        }
    }

    pub fn validate_identity(
        &self,
        run_id: &str,
        team_uri: &str,
        definition_hash: &str,
    ) -> anyhow::Result<()> {
        ensure!(
            self.schema_version == CONTINUATION_SCHEMA_VERSION,
            "unsupported continuation schema {}",
            self.schema_version
        );
        ensure!(self.run_id == run_id, "continuation run identity mismatch");
        ensure!(
            self.team_uri == team_uri,
            "continuation team identity mismatch"
        );
        ensure!(
            self.definition_hash == definition_hash,
            "continuation definition identity mismatch"
        );
        Ok(())
    }

    pub fn apply_guard_observation(
        &mut self,
        request: &GuardRequest,
        value: bool,
    ) -> anyhow::Result<()> {
        ensure!(
            request.run_id == self.run_id,
            "guard observation run identity mismatch"
        );
        self.guard_results.insert(request.key.clone(), value);
        self.event(
            "guardObserved",
            Some(request.node_id.clone()),
            Some(hash_json(&Value::Bool(value))),
        );
        Ok(())
    }

    pub fn apply_effect_observation(
        &mut self,
        observation: ActivityObservation,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let pending = self
            .pending_effects
            .get(&observation.effect_key)
            .cloned()
            .with_context(|| format!("effect {} is not pending", observation.effect_key))?;
        ensure!(
            observation.input_hash == pending.request.input_hash,
            "effect observation input hash mismatch"
        );
        ensure!(
            observation.output_hash == hash_json(&observation.output),
            "effect observation output hash mismatch"
        );
        ensure!(
            observation.observed_at <= now,
            "effect observation is dated in the future"
        );
        if let Some(limit) = pending.request.max_output_bytes {
            let output_size = serde_json::to_vec(&observation.output)?.len() as u64;
            ensure!(
                output_size <= limit,
                "effect output is {output_size} bytes; node limit is {limit}"
            );
        }
        self.pending_effects
            .remove(&observation.effect_key)
            .context("validated effect disappeared before it could be applied")?;

        self.event(
            "effectObserved",
            Some(pending.request.node_id.clone()),
            Some(observation.output_hash.clone()),
        );

        match pending.purpose {
            EffectPurpose::Node(mut token) => {
                let node = pending.request.node_id;
                token.value = observation.output;
                token.disposition = observation.disposition;
                token.phase = TokenPhase::Route;
                token.source_label = pending.request.node_label;
                if token.disposition == "completed" {
                    if let Some(compensation) = pending.compensation {
                        self.compensations.push(CompensationRecord {
                            effect_key: observation.effect_key,
                            node_id: node,
                            activity: compensation,
                            input: pending.request.input,
                            output: token.value.clone(),
                        });
                    }
                    self.queue.push_back(token);
                    return Ok(());
                }

                if let Some(retry) = pending.retry {
                    let retryable = retry.retry_on.is_empty()
                        && matches!(
                            token.disposition.as_str(),
                            "failed" | "timedOut" | "resourceExhausted"
                        )
                        || retry.retry_on.contains(&token.disposition);
                    if retryable && token.attempt < retry.max_attempts {
                        token.attempt += 1;
                        token.phase = TokenPhase::Enter;
                        let delay =
                            retry_delay_millis(&retry, &observation.effect_key, token.attempt);
                        let due_at = now + chrono::Duration::milliseconds(delay as i64);
                        let wait_id = format!("retry:{}:{}", token.id, token.attempt);
                        self.pending_waits.insert(
                            wait_id.clone(),
                            PendingWait {
                                id: wait_id.clone(),
                                node_id: node.clone(),
                                kind: SuspensionKind::Retry,
                                signal: None,
                                due_at: Some(due_at),
                                token,
                                approval: None,
                            },
                        );
                        self.event("retryScheduled", Some(node), Some(hash_text(&wait_id)));
                        return Ok(());
                    }
                }
                self.queue.push_back(token);
            }
            EffectPurpose::Lifecycle {
                mut token,
                scope_instance_id,
                stage,
                activity_index,
            } => {
                if observation.disposition == "completed" {
                    token.value = observation.output;
                    self.advance_scope_lifecycle(
                        token,
                        &scope_instance_id,
                        stage,
                        activity_index + 1,
                    )?;
                } else {
                    token.value = observation.output;
                    token.disposition = observation.disposition;
                    token.phase = TokenPhase::Route;
                    self.queue.push_back(token);
                }
            }
            EffectPurpose::Compensation(record) => {
                ensure!(
                    observation.disposition == "completed",
                    "compensation for node {} failed with disposition {}",
                    record.node_id,
                    observation.disposition
                );
                self.event(
                    "effectCompensated",
                    Some(record.node_id),
                    Some(observation.output_hash),
                );
                self.schedule_next_compensation();
            }
        }
        Ok(())
    }

    pub fn apply_command(
        &mut self,
        command: ResumeCommand,
        actor: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        ensure!(
            command.run_id() == self.run_id,
            "command run identity mismatch"
        );
        let nested_target = command
            .target_wait_id()
            .and_then(parse_nested_wait_id)
            .map(|(execution_id, wait_id)| (execution_id.to_owned(), wait_id.to_owned()));
        if let Some((execution_id, wait_id)) = nested_target {
            let nested_command = command.retarget_wait(wait_id)?;
            {
                let nested = self
                    .nested_workflows
                    .get_mut(&execution_id)
                    .with_context(|| format!("nested workflow {execution_id} is not active"))?;
                nested.apply_command(nested_command, actor, now)?;
            }
            self.event("nestedRunResumed", None, Some(hash_text(&execution_id)));
            return Ok(());
        }
        let mut resumed = true;
        match command {
            ResumeCommand::Signal {
                wait_id,
                name,
                payload,
                ..
            }
            | ResumeCommand::Event {
                wait_id,
                name,
                payload,
                ..
            } => {
                let wait = self.snapshot_wait(&wait_id)?;
                ensure!(
                    matches!(wait.kind, SuspensionKind::Signal | SuspensionKind::Event),
                    "wait {wait_id} does not accept signal/event commands"
                );
                if let Some(expected) = &wait.signal {
                    ensure!(expected == &name, "signal/event name mismatch");
                }
                self.resume_wait(wait, payload, "completed");
            }
            ResumeCommand::TimerFired {
                wait_id,
                expected_deadline,
                ..
            } => {
                let wait = self.snapshot_wait(&wait_id)?;
                ensure!(
                    matches!(
                        wait.kind,
                        SuspensionKind::Timer
                            | SuspensionKind::Retry
                            | SuspensionKind::Approval
                            | SuspensionKind::DurableYield
                    ),
                    "wait {wait_id} does not accept timerFired"
                );
                let deadline = wait.due_at.context("timer has no deadline")?;
                ensure!(deadline == expected_deadline, "timer deadline mismatch");
                ensure!(now >= deadline, "timer fired before its deadline");
                let disposition = if matches!(wait.kind, SuspensionKind::Approval) {
                    "timedOut"
                } else {
                    "completed"
                };
                self.resume_wait(wait, Value::Null, disposition);
            }
            ResumeCommand::Approve {
                wait_id,
                decision_id,
                payload,
                ..
            } => {
                let mut wait = self.snapshot_wait(&wait_id)?;
                let node_id = wait.node_id.clone();
                let decision_hash = hash_text(&decision_id);
                let approval = wait.approval.as_mut().context("wait is not an approval")?;
                ensure!(
                    approval.approvers.is_empty() || approval.approvers.contains(actor),
                    "actor is not an approver"
                );
                ensure!(
                    !approval
                        .decisions
                        .values()
                        .any(|recorded| recorded == &decision_id),
                    "approval decision already recorded"
                );
                ensure!(
                    approval
                        .decisions
                        .insert(actor.to_owned(), decision_id)
                        .is_none(),
                    "approver already decided this approval"
                );
                if approval.decisions.len() >= approval.quorum {
                    self.resume_wait(wait, payload, "completed");
                } else {
                    self.pending_waits.insert(wait_id, wait);
                    resumed = false;
                }
                self.event("approvalRecorded", Some(node_id), Some(decision_hash));
            }
            ResumeCommand::Reject {
                wait_id, payload, ..
            } => {
                let wait = self.snapshot_wait(&wait_id)?;
                let approval = wait.approval.as_ref().context("wait is not an approval")?;
                ensure!(
                    approval.approvers.is_empty() || approval.approvers.contains(actor),
                    "actor is not an approver"
                );
                self.resume_wait(wait, payload, "rejected");
            }
            ResumeCommand::Cancel { reason, .. } => {
                self.begin_cancellation(hash_json(&json!({
                    "actor": actor,
                    "reason": reason,
                })));
            }
            ResumeCommand::Retry { wait_id, .. } => {
                let wait = self.snapshot_wait(&wait_id)?;
                ensure!(
                    matches!(wait.kind, SuspensionKind::Retry),
                    "wait {wait_id} is not retryable"
                );
                self.resume_wait(wait, Value::Null, "completed");
            }
            ResumeCommand::Migrate {
                from_definition_hash,
                to_definition_hash,
                target_team_uri,
                state_patch,
                ..
            } => {
                ensure!(
                    self.definition_hash == from_definition_hash,
                    "migration source definition hash mismatch"
                );
                for operation in state_patch {
                    apply_patch_operation(&mut self.state, operation)?;
                }
                self.definition_hash = to_definition_hash;
                self.team_uri = target_team_uri;
                self.event("runMigrated", None, Some(hash_text(&self.definition_hash)));
            }
        }
        if resumed {
            self.event("runResumed", None, None);
        }
        Ok(())
    }

    /// Finds the earliest scheduler-fireable wait — a timer, retry, approval
    /// deadline, or durable yield — that is eligible at `now`, preserving
    /// every nested execution prefix required to route the command back to the
    /// exact child continuation. Ordering by wait identity breaks equal-time
    /// ties deterministically across map traversal and process restarts.
    pub(crate) fn earliest_due_timer(&self, now: DateTime<Utc>) -> Option<(String, DateTime<Utc>)> {
        let local = self
            .pending_waits
            .values()
            .filter(|wait| {
                matches!(
                    wait.kind,
                    SuspensionKind::Timer
                        | SuspensionKind::Retry
                        | SuspensionKind::Approval
                        | SuspensionKind::DurableYield
                )
            })
            .filter_map(|wait| {
                wait.due_at
                    .filter(|deadline| *deadline <= now)
                    .map(|deadline| (wait.id.clone(), deadline))
            });
        let nested = self
            .nested_workflows
            .iter()
            .filter_map(|(execution_id, child)| {
                child.earliest_due_timer(now).map(|(wait_id, deadline)| {
                    (format!("nested:{execution_id}:{wait_id}"), deadline)
                })
            });
        local
            .chain(nested)
            .min_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
    }

    fn snapshot_wait(&self, wait_id: &str) -> anyhow::Result<PendingWait> {
        self.pending_waits
            .get(wait_id)
            .cloned()
            .with_context(|| format!("wait {wait_id} is not pending"))
    }

    fn resume_wait(&mut self, wait: PendingWait, payload: Value, disposition: &str) {
        self.pending_waits.remove(&wait.id);
        let mut token = wait.token;
        if !payload.is_null() {
            token.value = payload;
        }
        disposition.clone_into(&mut token.disposition);
        token.phase = if matches!(
            wait.kind,
            SuspensionKind::Retry | SuspensionKind::DurableYield
        ) {
            TokenPhase::Enter
        } else {
            TokenPhase::Route
        };
        self.event(
            "waitSatisfied",
            Some(wait.node_id),
            Some(hash_text(&wait.id)),
        );
        self.queue.push_back(token);
    }

    fn schedule_next_compensation(&mut self) {
        let Some(record) = self.compensations.pop() else {
            self.event("runCancelled", None, None);
            return;
        };
        let effect_key = format!("compensate:{}", record.effect_key);
        let request = EffectRequest {
            effect_key: effect_key.clone(),
            run_id: self.run_id.clone(),
            execution_id: self.execution_id.clone(),
            node_id: record.node_id.clone(),
            node_label: format!("compensation:{}", record.node_id),
            activity: record.activity.clone(),
            input: json!({"forwardInput": record.input, "forwardOutput": record.output}),
            input_hash: String::new(),
            attempt: 1,
            authorities: Vec::new(),
            max_output_bytes: None,
            timeout_millis: None,
        }
        .with_input_hash();
        self.pending_effects.insert(
            effect_key,
            PendingEffect {
                request,
                purpose: EffectPurpose::Compensation(record),
                retry: None,
                compensation: None,
            },
        );
    }

    fn begin_cancellation(&mut self, detail_hash: String) {
        self.cancelling = true;
        self.queue.clear();
        self.joins.clear();
        self.pending_waits.clear();
        self.pending_effects.clear();
        self.nested_workflows.clear();
        self.event("cancellationRequested", None, Some(detail_hash));
        self.schedule_next_compensation();
    }

    fn advance_scope_lifecycle(
        &mut self,
        token: Token,
        scope_instance_id: &str,
        stage: LifecycleStage,
        activity_index: usize,
    ) -> anyhow::Result<()> {
        let instance = self
            .scopes
            .get(scope_instance_id)
            .with_context(|| format!("scope instance {scope_instance_id} disappeared"))?;
        let activities = match stage {
            LifecycleStage::Entry => &instance.entry_activities,
            LifecycleStage::Exit => &instance.exit_activities,
        };
        if activity_index < activities.len() {
            let mut next = token;
            next.node_id.clone_from(&activities[activity_index]);
            next.phase = TokenPhase::Enter;
            next.purpose = TokenPurpose::ScopeLifecycle {
                scope_instance_id: scope_instance_id.to_owned(),
                stage,
                activity_index,
            };
            self.queue.push_back(next);
            return Ok(());
        }

        match stage {
            LifecycleStage::Entry => self.start_scope_initials(&token, scope_instance_id),
            LifecycleStage::Exit => self.finish_scope_instance(token, scope_instance_id),
        }
    }

    fn start_scope_initials(
        &mut self,
        token: &Token,
        scope_instance_id: &str,
    ) -> anyhow::Result<()> {
        let instance = self
            .scopes
            .get(scope_instance_id)
            .with_context(|| format!("scope instance {scope_instance_id} disappeared"))?
            .clone();
        let remembered = self
            .history
            .get(&instance.scope_id)
            .map(|history| history.active_node_ids.clone())
            .filter(|nodes| !nodes.is_empty());
        let initial = remembered.unwrap_or(instance.initial_node_ids);
        if let Some(instance) = self.scopes.get_mut(scope_instance_id) {
            instance.active_tokens = initial.len();
        }
        for (index, node_id) in initial.into_iter().enumerate() {
            let mut child = if index == 0 {
                token.clone()
            } else {
                self.clone_token(token)
            };
            child.node_id = node_id;
            child.phase = TokenPhase::Enter;
            child.purpose = TokenPurpose::Normal;
            child.scope_stack.push(scope_instance_id.to_owned());
            self.queue.push_back(child);
        }
        Ok(())
    }

    fn finish_scope_instance(
        &mut self,
        mut token: Token,
        scope_instance_id: &str,
    ) -> anyhow::Result<()> {
        let instance = self
            .scopes
            .remove(scope_instance_id)
            .with_context(|| format!("scope instance {scope_instance_id} disappeared"))?;
        token.scope_stack.pop();
        token.node_id = instance.scope_node_id;
        token.value = instance.result;
        "completed".clone_into(&mut token.disposition);
        token.phase = TokenPhase::Route;
        token.purpose = TokenPurpose::Normal;
        self.queue.push_back(token);
        Ok(())
    }

    pub fn run_until_blocked(
        &mut self,
        definition: &Definition,
        now: DateTime<Utc>,
    ) -> anyhow::Result<KernelOutcome> {
        ensure!(
            self.schema_version == CONTINUATION_SCHEMA_VERSION,
            "unsupported continuation schema"
        );
        let mut steps = 0usize;
        while let Some(mut token) = self.queue.pop_front() {
            steps += 1;
            ensure!(steps <= 1_000_000, "workflow kernel step budget exhausted");
            match token.phase {
                TokenPhase::Enter => self.enter_node(definition, token, now)?,
                TokenPhase::Route => {
                    if !self.route_token(definition, &mut token, now)? {
                        self.queue.push_front(token);
                        break;
                    }
                }
            }
        }

        if !self.pending_effects.is_empty() {
            return Ok(KernelOutcome::Effects(
                self.pending_effects
                    .values()
                    .map(|pending| pending.request.clone())
                    .collect(),
            ));
        }
        let guards = self.pending_guard_requests(definition);
        if !guards.is_empty() {
            return Ok(KernelOutcome::Guards(guards));
        }
        if !self.queue.is_empty() {
            bail!("workflow queue is blocked without an effect, guard, or wait");
        }
        if !self.pending_waits.is_empty() {
            return Ok(KernelOutcome::Suspended(
                self.pending_waits.values().cloned().collect(),
            ));
        }
        if self.cancelling {
            return Ok(KernelOutcome::Cancelled);
        }
        if self.outputs.is_empty() {
            return Ok(KernelOutcome::Completed(Value::Null));
        }
        self.outputs.sort_by_key(|output| output.sequence);
        Ok(KernelOutcome::Completed(match self.outputs.as_slice() {
            [only] => only.value.clone(),
            many => Value::Array(many.iter().map(|item| item.value.clone()).collect()),
        }))
    }

    fn enter_node(
        &mut self,
        definition: &Definition,
        mut token: Token,
        now: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let node = definition
            .nodes
            .get(&token.node_id)
            .with_context(|| format!("token {} names missing node {}", token.id, token.node_id))?;
        self.event(
            "nodeEntered",
            Some(node.id.clone()),
            Some(hash_text(&token.id)),
        );
        self.record_active_node(node, &token);

        if matches!(token.purpose, TokenPurpose::Normal) && !token.joined {
            let Some(joined) = self.accept_join(definition, node, token)? else {
                return Ok(());
            };
            token = joined;
        }

        match token.purpose.clone() {
            TokenPurpose::ScopeLifecycle {
                scope_instance_id,
                stage,
                activity_index,
            } => {
                let NodeKind::Activity { activity } = &node.kind else {
                    bail!("scope lifecycle node {} is not an activity", node.id);
                };
                self.schedule_effect(
                    node,
                    activity.clone(),
                    token.clone(),
                    EffectPurpose::Lifecycle {
                        token,
                        scope_instance_id,
                        stage,
                        activity_index,
                    },
                );
                return Ok(());
            }
            TokenPurpose::Normal => {}
        }

        match &node.kind {
            NodeKind::Input | NodeKind::Gateway | NodeKind::Transform { .. } => {
                token.phase = TokenPhase::Route;
                "completed".clone_into(&mut token.disposition);
                token.source_label.clone_from(&node.label);
                self.queue.push_back(token);
            }
            NodeKind::Output => {
                self.outputs.push(SequencedValue {
                    sequence: self.next_event_sequence,
                    value: token.value.clone(),
                });
                self.event("outputProduced", Some(node.id.clone()), None);
                self.consume_scope_token(&token, None)?;
            }
            NodeKind::Final => {
                if token.scope_stack.is_empty() {
                    self.outputs.push(SequencedValue {
                        sequence: self.next_event_sequence,
                        value: token.value,
                    });
                    self.event("rootCompleted", Some(node.id.clone()), None);
                } else {
                    self.consume_scope_token(&token, Some(token.value.clone()))?;
                }
            }
            NodeKind::Wait {
                wait_for,
                signal,
                timer,
            } => {
                let wait_id = format!("wait:{}:{}", token.id, node.id);
                let mut due_at = None;
                if let Some(timer) = timer {
                    if let Some(delay) = timer.delay_millis {
                        due_at = Some(now + chrono::Duration::milliseconds(delay as i64));
                    } else if let Some(expression) = &timer.deadline {
                        let raw = evaluate_expression(
                            expression,
                            &token,
                            &self.original_input,
                            &self.state,
                        );
                        let computed = raw
                            .as_ref()
                            .ok()
                            .and_then(|value| value.as_str())
                            .and_then(|text| text.parse::<DateTime<Utc>>().ok());
                        match computed {
                            Some(deadline) => due_at = Some(deadline),
                            // A timer whose deadline cannot be computed must
                            // never park: the wait would carry no due_at and
                            // no command could ever resume it. The token
                            // routes a failed disposition instead, carrying a
                            // structured error record readable through the
                            // error value root.
                            None if matches!(wait_for, WaitKind::Timer) => {
                                let reason = match &raw {
                                    Err(error) => format!(
                                        "timer deadline expression failed to evaluate: {error:#}"
                                    ),
                                    Ok(value) if value.as_str().is_none() => format!(
                                        "timer deadline evaluated to a non-string value: {value}"
                                    ),
                                    Ok(value) => format!(
                                        "timer deadline {:?} is not an RFC 3339 timestamp",
                                        value.as_str().unwrap_or_default()
                                    ),
                                };
                                token.value = json!({
                                    "error": reason,
                                    "deadline": raw.unwrap_or(Value::Null),
                                });
                                "failed".clone_into(&mut token.disposition);
                                token.phase = TokenPhase::Route;
                                self.queue.push_back(token);
                                return Ok(());
                            }
                            None => {}
                        }
                    }
                }
                self.pending_waits.insert(
                    wait_id.clone(),
                    PendingWait {
                        id: wait_id.clone(),
                        node_id: node.id.clone(),
                        kind: match wait_for {
                            WaitKind::Event => SuspensionKind::Event,
                            WaitKind::Signal => SuspensionKind::Signal,
                            WaitKind::Timer => SuspensionKind::Timer,
                        },
                        signal: signal.clone(),
                        due_at,
                        token,
                        approval: None,
                    },
                );
                self.event(
                    "runSuspended",
                    Some(node.id.clone()),
                    Some(hash_text(&wait_id)),
                );
            }
            NodeKind::Approval {
                policy_id,
                quorum,
                approvers,
            } => {
                let wait_id = format!("approval:{}:{}", token.id, node.id);
                self.pending_waits.insert(
                    wait_id.clone(),
                    PendingWait {
                        id: wait_id.clone(),
                        node_id: node.id.clone(),
                        kind: SuspensionKind::Approval,
                        signal: Some(policy_id.clone()),
                        due_at: node.policy.timeout.as_ref().map(|timeout| {
                            now + chrono::Duration::milliseconds(timeout.after_millis as i64)
                        }),
                        token,
                        approval: Some(ApprovalState {
                            policy_id: policy_id.clone(),
                            quorum: *quorum,
                            approvers: approvers.iter().cloned().collect(),
                            decisions: BTreeMap::new(),
                        }),
                    },
                );
                self.event(
                    "approvalRequired",
                    Some(node.id.clone()),
                    Some(hash_text(&wait_id)),
                );
            }
            NodeKind::Activity { activity } => {
                self.schedule_effect(
                    node,
                    activity.clone(),
                    token.clone(),
                    EffectPurpose::Node(token),
                );
            }
            NodeKind::Scope { child_scope_id } => {
                self.enter_scope(definition, node, child_scope_id, &token)?;
            }
            NodeKind::History { history } => {
                self.restore_history(definition, node, *history, &token)?;
            }
        }
        Ok(())
    }

    fn schedule_effect(
        &mut self,
        node: &WorkflowNode,
        activity: ActivityReference,
        token: Token,
        purpose: EffectPurpose,
    ) {
        let correlation = token
            .epoch_stack
            .last()
            .cloned()
            .unwrap_or_else(|| "root".into());
        // The iteration counters are part of the effect identity: a bounded
        // cycle revisits the same node with a new input, and without them the
        // revisit collides with the continuation store's replay of the
        // previous iteration's observation (input hash mismatch). BTreeMap
        // iteration keeps the serialization canonical, and an unchanged token
        // still derives the same key, so crash-resume replay is preserved.
        let iterations = token
            .iterations
            .iter()
            .map(|(transition, count)| format!("{transition}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        let effect_key = hash_text(&format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
            self.run_id, self.execution_id, node.id, correlation, token.attempt, iterations
        ));
        if self.pending_effects.contains_key(&effect_key) {
            return;
        }
        let request = EffectRequest {
            effect_key: effect_key.clone(),
            run_id: self.run_id.clone(),
            execution_id: self.execution_id.clone(),
            node_id: node.id.clone(),
            node_label: node.label.clone(),
            activity,
            input: token.value,
            input_hash: String::new(),
            attempt: token.attempt,
            authorities: node.policy.authorities.clone(),
            max_output_bytes: node.policy.max_output_bytes,
            timeout_millis: node
                .policy
                .timeout
                .as_ref()
                .map(|timeout| timeout.after_millis),
        }
        .with_input_hash();
        self.pending_effects.insert(
            effect_key.clone(),
            PendingEffect {
                request,
                purpose,
                retry: node.policy.retry.clone(),
                compensation: node.policy.compensation.clone(),
            },
        );
        self.event("effectRequested", Some(node.id.clone()), Some(effect_key));
    }

    fn enter_scope(
        &mut self,
        definition: &Definition,
        node: &WorkflowNode,
        child_scope_id: &str,
        token: &Token,
    ) -> anyhow::Result<()> {
        let scope = definition.scopes.get(child_scope_id).with_context(|| {
            format!("scope node {} has missing child {child_scope_id}", node.id)
        })?;
        let instance_id = format!("scope:{}:{}", node.id, self.next_scope_sequence);
        self.next_scope_sequence += 1;
        self.scopes.insert(
            instance_id.clone(),
            ScopeInstance {
                id: instance_id.clone(),
                scope_id: scope.id.clone(),
                scope_node_id: node.id.clone(),
                kind: scope.kind,
                initial_node_ids: scope.initial_node_ids.clone(),
                entry_activities: scope.entry_activities.clone(),
                exit_activities: scope.exit_activities.clone(),
                history_mode: scope.history,
                active_tokens: 0,
                active_configuration: BTreeMap::new(),
                active_token_nodes: BTreeMap::new(),
                result: token.value.clone(),
            },
        );
        self.advance_scope_lifecycle(token.clone(), &instance_id, LifecycleStage::Entry, 0)
    }

    fn restore_history(
        &mut self,
        definition: &Definition,
        node: &WorkflowNode,
        mode: HistoryMode,
        token: &Token,
    ) -> anyhow::Result<()> {
        let scope = definition
            .scopes
            .get(&node.scope_id)
            .context("history node scope disappeared")?;
        let remembered = self.history.get(&scope.id).filter(|history| {
            matches!(mode, HistoryMode::Deep) || matches!(history.mode, HistoryMode::Shallow)
        });
        let targets = remembered
            .map(|history| history.active_node_ids.clone())
            .filter(|targets| !targets.is_empty())
            .unwrap_or_else(|| scope.initial_node_ids.clone());
        self.adjust_scope_fanout(token, targets.len());
        for (index, target) in targets.into_iter().enumerate() {
            let mut restored = if index == 0 {
                token.clone()
            } else {
                self.clone_token(token)
            };
            restored.node_id = target;
            restored.phase = TokenPhase::Enter;
            self.queue.push_back(restored);
        }
        Ok(())
    }

    fn record_active_node(&mut self, node: &WorkflowNode, token: &Token) {
        let Some(instance_id) = token.scope_stack.last() else {
            return;
        };
        let Some(instance) = self.scopes.get_mut(instance_id) else {
            return;
        };
        if matches!(&node.kind, NodeKind::Final) {
            return;
        }
        if let Some(previous) = instance
            .active_token_nodes
            .insert(token.id.clone(), node.id.clone())
            && let Some(count) = instance.active_configuration.get_mut(&previous)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                instance.active_configuration.remove(&previous);
            }
        }
        *instance
            .active_configuration
            .entry(node.id.clone())
            .or_insert(0) += 1;
    }

    fn consume_scope_token(&mut self, token: &Token, result: Option<Value>) -> anyhow::Result<()> {
        let Some(instance_id) = token.scope_stack.last() else {
            return Ok(());
        };
        let complete = {
            let instance = self
                .scopes
                .get_mut(instance_id)
                .with_context(|| format!("scope instance {instance_id} disappeared"))?;
            instance.active_tokens = instance.active_tokens.saturating_sub(1);
            instance.active_token_nodes.remove(&token.id);
            if let Some(result) = result {
                instance.result = result;
            }
            instance.active_tokens == 0
        };
        if !complete {
            return Ok(());
        }
        let instance = self.scopes[instance_id].clone();
        if !matches!(instance.history_mode, HistoryMode::None) {
            self.history.insert(
                instance.scope_id.clone(),
                ScopeHistory {
                    mode: instance.history_mode,
                    active_node_ids: instance.active_configuration.keys().cloned().collect(),
                },
            );
        }
        let mut lifecycle_token = token.clone();
        lifecycle_token.value = instance.result;
        self.advance_scope_lifecycle(lifecycle_token, instance_id, LifecycleStage::Exit, 0)
    }

    fn accept_join(
        &mut self,
        definition: &Definition,
        node: &WorkflowNode,
        token: Token,
    ) -> anyhow::Result<Option<Token>> {
        let Some(policy) = &node.join else {
            return Ok(Some(token));
        };
        if matches!(policy, JoinPolicy::Each { .. }) {
            return Ok(Some(token));
        }
        let correlation = correlation_key(policy, &token, &self.original_input, &self.state)?;
        let key = format!("{}\u{0}{correlation}", node.id);
        let expected = definition
            .transitions
            .values()
            .filter(|transition| transition.target_node_id == node.id)
            .map(|transition| transition.id.clone())
            .collect::<BTreeSet<_>>();
        let arrival_id = token
            .source_transition_id
            .clone()
            .unwrap_or_else(|| format!("entry:{}", token.id));
        let late_cancellation_hash = hash_text(&format!("late-arrival:{}:{}", node.id, arrival_id));
        let state = self.joins.entry(key.clone()).or_insert_with(|| JoinState {
            expected,
            arrivals: BTreeMap::new(),
            closed: false,
            accumulator: Value::Null,
        });
        ensure!(
            state.arrivals.insert(arrival_id, token.clone()).is_none(),
            "duplicate arrival at join {} for one correlation epoch",
            node.id
        );

        if state.closed {
            let late = match policy {
                JoinPolicy::Quorum { late, .. } | JoinPolicy::Race { late, .. } => late,
                _ => &LateArrival::Discard,
            };
            let complete = state.arrivals.len() >= state.expected.len();
            match late {
                LateArrival::Discard => {
                    self.decrement_scope_tokens(&token, 1);
                    if complete {
                        self.joins.remove(&key);
                    }
                    return Ok(None);
                }
                LateArrival::Cancel => {
                    self.decrement_scope_tokens(&token, 1);
                    self.begin_cancellation(late_cancellation_hash);
                    return Ok(None);
                }
                LateArrival::Error => bail!("late arrival at closed join {}", node.id),
            }
        }

        let fire = match policy {
            JoinPolicy::All { .. } => state.arrivals.len() >= state.expected.len(),
            JoinPolicy::Quorum { quorum, .. } => state.arrivals.len() >= *quorum,
            JoinPolicy::Race { .. } => !state.closed,
            JoinPolicy::Reduce { reducer, until, .. } => {
                state.accumulator = reduce_value(reducer, &state.accumulator, &token.value)?;
                let mut reduced = token.clone();
                reduced.value = state.accumulator.clone();
                evaluate_condition(until, &reduced, &self.original_input, &self.state)?
            }
            JoinPolicy::Each { .. } => true,
        };

        if !fire {
            return Ok(None);
        }
        state.closed = matches!(policy, JoinPolicy::Quorum { .. } | JoinPolicy::Race { .. });
        let mut arrivals = state.arrivals.values().cloned().collect::<Vec<_>>();
        arrivals.sort_by(|left, right| left.id.cmp(&right.id));
        let mut merged = arrivals
            .first()
            .cloned()
            .context("join fired without an arrival")?;
        merged.joined = true;
        let accumulator = state.accumulator.clone();
        let remove_state = !state.closed || state.arrivals.len() >= state.expected.len();
        merged.value = match policy {
            JoinPolicy::Race { .. } => token.value,
            JoinPolicy::Reduce { .. } => accumulator,
            _ => {
                let mut values = Map::new();
                for arrival in &arrivals {
                    ensure!(
                        !values.contains_key(&arrival.source_label),
                        "join {} has duplicate producer label {}",
                        node.id,
                        arrival.source_label
                    );
                    values.insert(arrival.source_label.clone(), arrival.value.clone());
                }
                Value::Object(values)
            }
        };
        if remove_state {
            self.joins.remove(&key);
        }
        self.decrement_scope_tokens(&merged, arrivals.len().saturating_sub(1));
        Ok(Some(merged))
    }

    fn route_token(
        &mut self,
        definition: &Definition,
        token: &mut Token,
        now: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let node = definition
            .nodes
            .get(&token.node_id)
            .with_context(|| format!("route source node {} is missing", token.node_id))?;
        let mut groups = definition
            .route_groups
            .values()
            .filter_map(|group| {
                let mut transitions = definition
                    .transitions
                    .values()
                    .filter(|transition| {
                        transition.source_node_id == node.id
                            && transition.route_group_id == group.id
                            && transition.trigger == token.disposition
                    })
                    .collect::<Vec<_>>();
                transitions.sort_by_key(|transition| (transition.priority, transition.id.as_str()));
                (!transitions.is_empty()).then_some((group, transitions))
            })
            .collect::<Vec<_>>();
        groups.sort_by_key(|(group, _)| group.id.as_str());
        if groups.is_empty() {
            if token.disposition == "completed" {
                self.outputs.push(SequencedValue {
                    sequence: self.next_event_sequence,
                    value: token.value.clone(),
                });
                self.consume_scope_token(token, Some(token.value.clone()))?;
                return Ok(true);
            }
            bail!(
                "node {} produced disposition {} with no matching route group",
                node.id,
                token.disposition
            );
        }

        for (_, transitions) in &groups {
            for transition in transitions {
                if contains_rholang(transition.guard.as_ref()) {
                    let key = guard_key(&self.run_id, &self.execution_id, token, transition);
                    if !self.guard_results.contains_key(&key) {
                        return Ok(false);
                    }
                }
            }
        }

        let mut selected = Vec::<(&WorkflowTransition, String)>::new();
        for (group, transitions) in groups {
            let mut matches = Vec::new();
            for transition in transitions {
                let truth = if contains_rholang(transition.guard.as_ref()) {
                    let key = guard_key(&self.run_id, &self.execution_id, token, transition);
                    self.guard_results[&key]
                } else {
                    transition.guard.as_ref().map_or(Ok(true), |condition| {
                        evaluate_condition(condition, token, &self.original_input, &self.state)
                    })?
                };
                if truth {
                    matches.push(transition);
                    if matches!(group.mode, RoutingMode::FirstMatch) {
                        break;
                    }
                }
            }
            if matches.is_empty() {
                match group.no_match {
                    NoMatch::Complete => {
                        self.outputs.push(SequencedValue {
                            sequence: self.next_event_sequence,
                            value: token.value.clone(),
                        });
                        self.consume_scope_token(token, Some(token.value.clone()))?;
                    }
                    NoMatch::Skip => self.consume_scope_token(token, None)?,
                    NoMatch::Fail => bail!("route group {} matched no transition", group.id),
                }
            } else {
                selected.extend(
                    matches
                        .into_iter()
                        .map(|transition| (transition, group.id.clone())),
                );
            }
        }

        if selected.is_empty() {
            return Ok(true);
        }
        let mut selected_per_group = BTreeMap::<String, usize>::new();
        for (_, group_id) in &selected {
            *selected_per_group.entry(group_id.clone()).or_insert(0) += 1;
        }
        let mut fork_epochs = BTreeMap::new();
        for (group_id, count) in selected_per_group {
            if count <= 1 {
                continue;
            }
            let generation = token.fork_generations.entry(group_id.clone()).or_insert(0);
            *generation += 1;
            let epoch = hash_text(&format!(
                "{}\u{0}{}\u{0}{}\u{0}{}",
                self.run_id,
                token.epoch_stack.last().map_or("root", String::as_str),
                group_id,
                generation
            ));
            fork_epochs.insert(group_id, epoch);
        }
        self.adjust_scope_fanout(token, selected.len());
        for (index, (transition, group_id)) in selected.into_iter().enumerate() {
            let mut child = if index == 0 {
                token.clone()
            } else {
                self.clone_token(token)
            };
            if let Some(epoch) = fork_epochs.get(&group_id) {
                child.epoch_stack.push(epoch.clone());
                child.correlations.insert(group_id.clone(), epoch.clone());
            }
            if let Some(loop_policy) = &transition.loop_policy {
                match loop_policy {
                    LoopPolicy::Bounded { max_iterations } => {
                        let count = child.iterations.entry(transition.id.clone()).or_insert(0);
                        if *count >= *max_iterations {
                            "iterationLimit".clone_into(&mut child.disposition);
                            child.phase = TokenPhase::Route;
                            self.queue.push_back(child);
                            continue;
                        }
                        *count += 1;
                    }
                    LoopPolicy::Durable { .. } => {
                        child.node_id.clone_from(&transition.target_node_id);
                        child.phase = TokenPhase::Enter;
                        child.joined = false;
                        child.source_transition_id = Some(transition.id.clone());
                        child.source_label.clone_from(&node.label);
                        let wait_id = format!("yield:{}:{}", child.id, transition.id);
                        self.pending_waits.insert(
                            wait_id.clone(),
                            PendingWait {
                                id: wait_id.clone(),
                                node_id: node.id.clone(),
                                kind: SuspensionKind::DurableYield,
                                signal: None,
                                due_at: Some(now),
                                token: child,
                                approval: None,
                            },
                        );
                        self.event(
                            "durableYield",
                            Some(node.id.clone()),
                            Some(hash_text(&wait_id)),
                        );
                        continue;
                    }
                }
            }
            child.node_id.clone_from(&transition.target_node_id);
            child.phase = TokenPhase::Enter;
            child.joined = false;
            child.source_transition_id = Some(transition.id.clone());
            child.source_label.clone_from(&node.label);
            self.event(
                "transitionTaken",
                Some(node.id.clone()),
                Some(hash_text(&transition.id)),
            );
            self.queue.push_back(child);
        }
        for transition in definition.transitions.values().filter(|transition| {
            transition.source_node_id == node.id && transition.trigger == token.disposition
        }) {
            self.guard_results.remove(&guard_key(
                &self.run_id,
                &self.execution_id,
                token,
                transition,
            ));
        }
        Ok(true)
    }

    fn adjust_scope_fanout(&mut self, token: &Token, outputs: usize) {
        if outputs <= 1 {
            return;
        }
        if let Some(instance_id) = token.scope_stack.last()
            && let Some(instance) = self.scopes.get_mut(instance_id)
        {
            instance.active_tokens += outputs - 1;
        }
    }

    fn decrement_scope_tokens(&mut self, token: &Token, amount: usize) {
        if amount == 0 {
            return;
        }
        if let Some(instance_id) = token.scope_stack.last()
            && let Some(instance) = self.scopes.get_mut(instance_id)
        {
            instance.active_tokens = instance.active_tokens.saturating_sub(amount);
        }
    }

    fn pending_guard_requests(&self, definition: &Definition) -> Vec<GuardRequest> {
        let mut requests = Vec::new();
        for token in self
            .queue
            .iter()
            .filter(|token| matches!(token.phase, TokenPhase::Route))
        {
            for transition in definition.transitions.values().filter(|transition| {
                transition.source_node_id == token.node_id
                    && transition.trigger == token.disposition
                    && contains_rholang(transition.guard.as_ref())
            }) {
                let key = guard_key(&self.run_id, &self.execution_id, token, transition);
                if self.guard_results.contains_key(&key) {
                    continue;
                }
                requests.push(GuardRequest {
                    key,
                    run_id: self.run_id.clone(),
                    execution_id: self.execution_id.clone(),
                    node_id: token.node_id.clone(),
                    transition_id: transition.id.clone(),
                    value: token.value.clone(),
                    input: self.original_input.clone(),
                    state: self.state.clone(),
                });
            }
        }
        requests
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Token {
    pub id: String,
    pub node_id: String,
    pub value: Value,
    pub disposition: String,
    pub source_label: String,
    pub source_transition_id: Option<String>,
    pub iterations: BTreeMap<String, u64>,
    pub fork_generations: BTreeMap<String, u64>,
    pub correlations: BTreeMap<String, String>,
    pub epoch_stack: Vec<String>,
    pub scope_stack: Vec<String>,
    pub attempt: u32,
    pub joined: bool,
    pub phase: TokenPhase,
    pub purpose: TokenPurpose,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenPhase {
    Enter,
    Route,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TokenPurpose {
    Normal,
    ScopeLifecycle {
        scope_instance_id: String,
        stage: LifecycleStage,
        activity_index: usize,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleStage {
    Entry,
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinState {
    pub expected: BTreeSet<String>,
    pub arrivals: BTreeMap<String, Token>,
    pub closed: bool,
    pub accumulator: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeInstance {
    pub id: String,
    pub scope_id: String,
    pub scope_node_id: String,
    pub kind: ScopeKind,
    pub initial_node_ids: Vec<String>,
    pub entry_activities: Vec<String>,
    pub exit_activities: Vec<String>,
    pub history_mode: HistoryMode,
    pub active_tokens: usize,
    pub active_configuration: BTreeMap<String, usize>,
    pub active_token_nodes: BTreeMap<String, String>,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeHistory {
    pub mode: HistoryMode,
    pub active_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompensationRecord {
    pub effect_key: String,
    pub node_id: String,
    pub activity: ActivityReference,
    pub input: Value,
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingEffect {
    pub request: EffectRequest,
    pub purpose: EffectPurpose,
    pub retry: Option<super::workflow::RetryPolicy>,
    pub compensation: Option<ActivityReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EffectPurpose {
    Node(Token),
    Lifecycle {
        token: Token,
        scope_instance_id: String,
        stage: LifecycleStage,
        activity_index: usize,
    },
    Compensation(CompensationRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectRequest {
    pub effect_key: String,
    pub run_id: String,
    #[serde(default = "default_execution_id")]
    pub execution_id: String,
    pub node_id: String,
    pub node_label: String,
    pub activity: ActivityReference,
    pub input: Value,
    pub input_hash: String,
    pub attempt: u32,
    pub authorities: Vec<String>,
    pub max_output_bytes: Option<u64>,
    pub timeout_millis: Option<u64>,
}

impl EffectRequest {
    fn with_input_hash(mut self) -> Self {
        self.input_hash = hash_json(&self.input);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityObservation {
    pub effect_key: String,
    pub input_hash: String,
    pub output: Value,
    pub output_hash: String,
    pub disposition: String,
    pub provider: String,
    pub implementation_hash: String,
    pub observed_at: DateTime<Utc>,
    pub cost: Option<Value>,
    pub deploy_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuardRequest {
    pub key: String,
    pub run_id: String,
    #[serde(default = "default_execution_id")]
    pub execution_id: String,
    pub node_id: String,
    pub transition_id: String,
    pub value: Value,
    pub input: Value,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingWait {
    pub id: String,
    pub node_id: String,
    pub kind: SuspensionKind,
    pub signal: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub token: Token,
    pub approval: Option<ApprovalState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuspensionKind {
    Approval,
    DurableYield,
    Event,
    Retry,
    Signal,
    Timer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalState {
    pub policy_id: String,
    pub quorum: usize,
    pub approvers: BTreeSet<String>,
    pub decisions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SequencedValue {
    pub sequence: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KernelEvent {
    pub sequence: u64,
    pub event_type: String,
    pub node_id: Option<String>,
    pub detail_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub enum KernelOutcome {
    Effects(Vec<EffectRequest>),
    Guards(Vec<GuardRequest>),
    Suspended(Vec<PendingWait>),
    Completed(Value),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResumeCommand {
    Signal {
        run_id: String,
        wait_id: String,
        name: String,
        payload: Value,
    },
    Event {
        run_id: String,
        wait_id: String,
        name: String,
        payload: Value,
    },
    TimerFired {
        run_id: String,
        wait_id: String,
        expected_deadline: DateTime<Utc>,
    },
    Approve {
        run_id: String,
        wait_id: String,
        decision_id: String,
        payload: Value,
    },
    Reject {
        run_id: String,
        wait_id: String,
        payload: Value,
    },
    Cancel {
        run_id: String,
        reason: String,
    },
    Retry {
        run_id: String,
        wait_id: String,
    },
    Migrate {
        run_id: String,
        from_definition_hash: String,
        to_definition_hash: String,
        target_team_uri: String,
        state_patch: Vec<StatePatchOperation>,
    },
}

impl ResumeCommand {
    pub fn run_id(&self) -> &str {
        match self {
            Self::Signal { run_id, .. }
            | Self::Event { run_id, .. }
            | Self::TimerFired { run_id, .. }
            | Self::Approve { run_id, .. }
            | Self::Reject { run_id, .. }
            | Self::Cancel { run_id, .. }
            | Self::Retry { run_id, .. }
            | Self::Migrate { run_id, .. } => run_id,
        }
    }

    /// Public authorization class committed by the on-chain command claim.
    ///
    /// The encrypted command remains the source of truth for its payload.  This
    /// deliberately small discriminator lets the registry grant an approver
    /// authority for one approval wait without granting cancellation,
    /// migration, retry, or another wait's approval authority.
    pub const fn authorization_class(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Signal { .. } => "signal",
            Self::Event { .. } => "event",
            Self::TimerFired { .. } => "timerFired",
            Self::Cancel { .. } => "cancel",
            Self::Retry { .. } => "retry",
            Self::Migrate { .. } => "migrate",
        }
    }

    /// Wait identity used by the registry for capability narrowing.  Commands
    /// that operate on the run as a whole intentionally have no wait identity.
    pub fn target_wait_id(&self) -> Option<&str> {
        match self {
            Self::Signal { wait_id, .. }
            | Self::Event { wait_id, .. }
            | Self::TimerFired { wait_id, .. }
            | Self::Approve { wait_id, .. }
            | Self::Reject { wait_id, .. }
            | Self::Retry { wait_id, .. } => Some(wait_id),
            Self::Cancel { .. } | Self::Migrate { .. } => None,
        }
    }

    fn retarget_wait(self, wait_id: String) -> anyhow::Result<Self> {
        Ok(match self {
            Self::Signal {
                run_id,
                name,
                payload,
                ..
            } => Self::Signal {
                run_id,
                wait_id,
                name,
                payload,
            },
            Self::Event {
                run_id,
                name,
                payload,
                ..
            } => Self::Event {
                run_id,
                wait_id,
                name,
                payload,
            },
            Self::TimerFired {
                run_id,
                expected_deadline,
                ..
            } => Self::TimerFired {
                run_id,
                wait_id,
                expected_deadline,
            },
            Self::Approve {
                run_id,
                decision_id,
                payload,
                ..
            } => Self::Approve {
                run_id,
                wait_id,
                decision_id,
                payload,
            },
            Self::Reject {
                run_id, payload, ..
            } => Self::Reject {
                run_id,
                wait_id,
                payload,
            },
            Self::Retry { run_id, .. } => Self::Retry { run_id, wait_id },
            Self::Cancel { .. } | Self::Migrate { .. } => {
                bail!("run-level command cannot target a nested wait")
            }
        })
    }
}

fn parse_nested_wait_id(wait_id: &str) -> Option<(&str, &str)> {
    wait_id.strip_prefix("nested:")?.split_once(':')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StatePatchOperation {
    Set { path: Vec<String>, value: Value },
    Remove { path: Vec<String> },
}

fn apply_patch_operation(state: &mut Value, operation: StatePatchOperation) -> anyhow::Result<()> {
    let (path, replacement) = match operation {
        StatePatchOperation::Set { path, value } => (path, Some(value)),
        StatePatchOperation::Remove { path } => (path, None),
    };
    ensure!(!path.is_empty(), "state migration path cannot be empty");
    let mut current = state;
    for segment in &path[..path.len() - 1] {
        current = current
            .as_object_mut()
            .with_context(|| format!("state migration path segment {segment} is not a record"))?
            .get_mut(segment)
            .with_context(|| format!("state migration path segment {segment} is missing"))?;
    }
    let key = path.last().context("state migration path disappeared")?;
    let object = current
        .as_object_mut()
        .context("state migration target is not a record")?;
    match replacement {
        Some(value) => {
            object.insert(key.clone(), value);
        }
        None => {
            ensure!(
                object.remove(key).is_some(),
                "state migration remove target is missing"
            );
        }
    }
    Ok(())
}

fn correlation_key(
    policy: &JoinPolicy,
    token: &Token,
    input: &Value,
    state: &Value,
) -> anyhow::Result<String> {
    let correlation = match policy {
        JoinPolicy::All { correlation }
        | JoinPolicy::Each { correlation }
        | JoinPolicy::Quorum { correlation, .. }
        | JoinPolicy::Race { correlation, .. }
        | JoinPolicy::Reduce { correlation, .. } => correlation,
    };
    Ok(match correlation {
        CorrelationPolicy::Root => "root".to_owned(),
        CorrelationPolicy::RouteGroup { route_group_id } => token
            .correlations
            .get(route_group_id)
            .cloned()
            .with_context(|| format!("token has no epoch for route group {route_group_id}"))?,
        CorrelationPolicy::Key { value } => {
            hash_json(&evaluate_expression(value, token, input, state)?)
        }
    })
}

fn contains_rholang(condition: Option<&Condition>) -> bool {
    fn visit(condition: &Condition) -> bool {
        match condition {
            Condition::Rholang { .. } => true,
            Condition::All { operands } | Condition::Any { operands } => operands.iter().any(visit),
            Condition::Not { operand } => visit(operand),
            _ => false,
        }
    }
    condition.is_some_and(visit)
}

fn evaluate_condition(
    condition: &Condition,
    token: &Token,
    input: &Value,
    state: &Value,
) -> anyhow::Result<bool> {
    match condition {
        Condition::Literal { value } => Ok(*value),
        Condition::All { operands } => {
            for operand in operands {
                if !evaluate_condition(operand, token, input, state)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Condition::Any { operands } => {
            for operand in operands {
                if evaluate_condition(operand, token, input, state)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Condition::Not { operand } => Ok(!evaluate_condition(operand, token, input, state)?),
        Condition::Exists { expression } => {
            Ok(!evaluate_expression(expression, token, input, state)?.is_null())
        }
        Condition::Compare {
            left,
            operator,
            right,
        } => compare_values(
            &evaluate_expression(left, token, input, state)?,
            *operator,
            &evaluate_expression(right, token, input, state)?,
        ),
        Condition::Rholang { .. } => {
            bail!("Rholang condition requires an on-chain guard observation")
        }
    }
}

fn evaluate_expression(
    expression: &ValueExpression,
    token: &Token,
    input: &Value,
    state: &Value,
) -> anyhow::Result<Value> {
    match expression {
        ValueExpression::Literal { value } => Ok(value.clone()),
        ValueExpression::Path { value } => evaluate_path(value, token, input, state).cloned(),
    }
}

fn evaluate_path<'a>(
    path: &ValuePath,
    token: &'a Token,
    input: &'a Value,
    state: &'a Value,
) -> anyhow::Result<&'a Value> {
    let mut value = match path.root {
        ValueRoot::Input => input,
        ValueRoot::Output | ValueRoot::Error => &token.value,
        ValueRoot::State => state,
    };
    for segment in &path.path {
        value = value
            .as_object()
            .and_then(|object| object.get(segment))
            .with_context(|| format!("value path segment {segment} is missing"))?;
    }
    Ok(value)
}

fn compare_values(left: &Value, operator: Comparison, right: &Value) -> anyhow::Result<bool> {
    Ok(match operator {
        Comparison::Eq => left == right,
        Comparison::Ne => left != right,
        Comparison::Gt | Comparison::Ge | Comparison::Lt | Comparison::Le => {
            let ordering = match (left, right) {
                (Value::Number(left), Value::Number(right)) => left
                    .as_f64()
                    .partial_cmp(&right.as_f64())
                    .context("numbers are not comparable")?,
                (Value::String(left), Value::String(right)) => left.cmp(right),
                _ => bail!("ordered comparison requires two numbers or two strings"),
            };
            match operator {
                Comparison::Gt => ordering.is_gt(),
                Comparison::Ge => ordering.is_ge(),
                Comparison::Lt => ordering.is_lt(),
                Comparison::Le => ordering.is_le(),
                _ => false,
            }
        }
    })
}

fn reduce_value(
    operation: &ReduceOperation,
    accumulator: &Value,
    value: &Value,
) -> anyhow::Result<Value> {
    match operation {
        ReduceOperation::Collect => {
            let mut result = accumulator.as_array().cloned().unwrap_or_default();
            result.push(value.clone());
            Ok(Value::Array(result))
        }
        ReduceOperation::First => Ok(if accumulator.is_null() {
            value.clone()
        } else {
            accumulator.clone()
        }),
        ReduceOperation::Last => Ok(value.clone()),
        ReduceOperation::MergeRecords => {
            let mut result = accumulator.as_object().cloned().unwrap_or_default();
            for (key, item) in value.as_object().context("mergeRecords requires records")? {
                ensure!(
                    !result.contains_key(key),
                    "mergeRecords key conflict at {key}"
                );
                result.insert(key.clone(), item.clone());
            }
            Ok(Value::Object(result))
        }
        ReduceOperation::Sum | ReduceOperation::Min | ReduceOperation::Max => {
            let number = value
                .as_f64()
                .context("numeric reducer received a non-number")?;
            if accumulator.is_null() {
                return Ok(value.clone());
            }
            let current = accumulator
                .as_f64()
                .context("numeric reducer accumulator is not a number")?;
            let result = match operation {
                ReduceOperation::Sum => current + number,
                ReduceOperation::Min => current.min(number),
                ReduceOperation::Max => current.max(number),
                _ => unreachable!(),
            };
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .context("numeric reducer produced a non-finite number")
        }
    }
}

fn retry_delay_millis(
    policy: &super::workflow::RetryPolicy,
    effect_key: &str,
    attempt: u32,
) -> u64 {
    let exponent = attempt.saturating_sub(2);
    let multiplier = u128::from(policy.backoff_multiplier_millis).pow(exponent);
    let raw =
        u128::from(policy.initial_delay_millis).saturating_mul(multiplier) / 1000u128.pow(exponent);
    let capped = u64::try_from(raw.min(u128::from(policy.max_delay_millis)))
        .expect("retry delay is capped at a u64 policy value");
    if !policy.deterministic_jitter || capped == 0 {
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

fn guard_key(
    run_id: &str,
    execution_id: &str,
    token: &Token,
    transition: &WorkflowTransition,
) -> String {
    hash_text(&format!(
        "{run_id}\u{0}{execution_id}\u{0}{}\u{0}{}\u{0}{}",
        token.id, token.attempt, transition.id
    ))
}

pub fn hash_json(value: &Value) -> String {
    hash_text(&canonical_json(value))
}

pub fn hash_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn canonical_json(value: &Value) -> String {
    fn canonicalize(value: &Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
            Value::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), canonicalize(value)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            scalar => scalar.clone(),
        }
    }
    serde_json::to_string(&canonicalize(value)).expect("JSON value serialization is infallible")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::agents_teams::workflow::WorkflowDefinition;

    fn definition(nodes: Value, groups: Value, transitions: Value) -> WorkflowDefinition {
        let mut definition = json!({
            "schemaVersion":"2.0",
            "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"legacy"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{},
            "policies":{"defaultExecution":"durable","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{},
            "scopes":{"root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"}},
            "transitions":{},
            "types":{}
        });
        definition["nodes"] = nodes;
        definition["routeGroups"] = groups;
        definition["transitions"] = transitions;
        WorkflowDefinition::new(definition).expect("test definition should parse")
    }

    fn observation(
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

    fn fork_join_definition(join: &Value) -> WorkflowDefinition {
        definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "a":{"id":"a","label":"A","scopeId":"root","kind":"gateway"},
                "b":{"id":"b","label":"B","scopeId":"root","kind":"gateway"},
                "join":{"id":"join","label":"Join","scopeId":"root","join":join,"kind":"gateway"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "fork":{"id":"fork","mode":"allMatches","noMatch":"fail"},
                "a":{"id":"a","mode":"allMatches","noMatch":"fail"},
                "b":{"id":"b","mode":"allMatches","noMatch":"fail"},
                "done":{"id":"done","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "to-a":{"id":"to-a","sourceNodeId":"in","targetNodeId":"a","trigger":"completed","priority":0,"routeGroupId":"fork"},
                "to-b":{"id":"to-b","sourceNodeId":"in","targetNodeId":"b","trigger":"completed","priority":1,"routeGroupId":"fork"},
                "a-join":{"id":"a-join","sourceNodeId":"a","targetNodeId":"join","trigger":"completed","priority":0,"routeGroupId":"a"},
                "b-join":{"id":"b-join","sourceNodeId":"b","targetNodeId":"join","trigger":"completed","priority":0,"routeGroupId":"b"},
                "join-out":{"id":"join-out","sourceNodeId":"join","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"done"}
            }),
        )
    }

    fn run_to_completion(
        definition: &WorkflowDefinition,
        input: Value,
    ) -> anyhow::Result<KernelOutcome> {
        definition.parsed().validate_publishable()?;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            input,
        )?;
        continuation.run_until_blocked(definition.parsed(), DateTime::<Utc>::UNIX_EPOCH)
    }

    #[test]
    fn cyclic_activity_iterations_get_distinct_effect_keys() {
        // A bounded cycle re-invokes the same activity node with a new input
        // each iteration. Every invocation must mint a distinct effect key,
        // otherwise the durable orchestrator's continuation store replays the
        // previous iteration's observation against the new input and the run
        // dies with an input-hash mismatch instead of looping.
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "act":{"id":"act","label":"Act","scopeId":"root","kind":"activity","activity":{"kind":"agent","agentId":"agent-1","agentVersion":"v1","protocol":"simple-v1"}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "next":{"id":"next","mode":"firstMatch","noMatch":"fail"},
                "limit":{"id":"limit","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"act","trigger":"completed","priority":0,"routeGroupId":"start"},
                "revise":{"id":"revise","sourceNodeId":"act","targetNodeId":"act","trigger":"completed","guard":{"kind":"literal","value":true},"priority":0,"routeGroupId":"next","loop":{"maxIterations":2}},
                "done":{"id":"done","sourceNodeId":"act","targetNodeId":"out","trigger":"iterationLimit","priority":0,"routeGroupId":"limit"}
            }),
        );
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!("question"),
        )
        .expect("continuation should start");

        let mut effect_keys = Vec::new();
        let final_output = loop {
            match continuation
                .run_until_blocked(definition.parsed(), Utc::now())
                .expect("kernel should drive")
            {
                KernelOutcome::Effects(requests) => {
                    for request in requests {
                        effect_keys.push(request.effect_key.clone());
                        continuation
                            .apply_effect_observation(
                                observation(
                                    &request,
                                    json!({"iteration": effect_keys.len()}),
                                    "completed",
                                    Utc::now(),
                                ),
                                Utc::now(),
                            )
                            .expect("observation should apply");
                    }
                }
                KernelOutcome::Completed(value) => break value,
                other => panic!("unexpected kernel outcome: {other:?}"),
            }
        };

        // maxIterations 2 admits two revise traversals: three invocations.
        assert_eq!(effect_keys.len(), 3, "three activity invocations expected");
        let distinct: std::collections::BTreeSet<_> = effect_keys.iter().collect();
        assert_eq!(
            distinct.len(),
            effect_keys.len(),
            "every cycle iteration must mint a distinct effect key: {effect_keys:?}"
        );
        assert_eq!(final_output, json!({"iteration": 3}));
    }

    #[test]
    fn first_match_and_bounded_cycle_are_deterministic() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "loop":{"id":"loop","label":"Loop","scopeId":"root","kind":"gateway"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"firstMatch","noMatch":"fail"},
                "again":{"id":"again","mode":"firstMatch","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"loop","trigger":"completed","priority":0,"routeGroupId":"start"},
                "again":{"id":"again","sourceNodeId":"loop","targetNodeId":"loop","trigger":"completed","guard":{"kind":"literal","value":false},"priority":0,"routeGroupId":"again","loop":{"maxIterations":2}},
                "done":{"id":"done","sourceNodeId":"loop","targetNodeId":"out","trigger":"completed","guard":{"kind":"literal","value":true},"priority":1,"routeGroupId":"again"}
            }),
        );
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!({"answer":42}),
        )
        .expect("continuation should start");
        let outcome = continuation
            .run_until_blocked(definition.parsed(), Utc::now())
            .expect("kernel should complete");
        assert!(
            matches!(outcome, KernelOutcome::Completed(value) if value == json!({"answer":42}))
        );
    }

    #[test]
    fn all_match_fanout_correlates_and_joins_per_epoch() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "a":{"id":"a","label":"A","scopeId":"root","kind":"gateway"},
                "b":{"id":"b","label":"B","scopeId":"root","kind":"gateway"},
                "join":{"id":"join","label":"Join","scopeId":"root","join":{"mode":"all","correlation":{"kind":"routeGroup","routeGroupId":"fork"}},"kind":"gateway"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "fork":{"id":"fork","mode":"allMatches","noMatch":"fail"},
                "a":{"id":"a","mode":"allMatches","noMatch":"fail"},
                "b":{"id":"b","mode":"allMatches","noMatch":"fail"},
                "joined":{"id":"joined","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "to-a":{"id":"to-a","sourceNodeId":"in","targetNodeId":"a","trigger":"completed","priority":0,"routeGroupId":"fork"},
                "to-b":{"id":"to-b","sourceNodeId":"in","targetNodeId":"b","trigger":"completed","priority":1,"routeGroupId":"fork"},
                "a-join":{"id":"a-join","sourceNodeId":"a","targetNodeId":"join","trigger":"completed","priority":0,"routeGroupId":"a"},
                "b-join":{"id":"b-join","sourceNodeId":"b","targetNodeId":"join","trigger":"completed","priority":0,"routeGroupId":"b"},
                "join-out":{"id":"join-out","sourceNodeId":"join","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"joined"}
            }),
        );
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!(7),
        )
        .expect("continuation should start");
        let outcome = continuation
            .run_until_blocked(definition.parsed(), Utc::now())
            .expect("kernel should complete");
        assert!(
            matches!(outcome, KernelOutcome::Completed(Value::Object(values)) if values == Map::from_iter([("A".into(), json!(7)), ("B".into(), json!(7))]))
        );
        assert!(continuation.joins.is_empty());
    }

    #[test]
    fn each_quorum_race_and_reduce_joins_have_distinct_deterministic_semantics() {
        let each = fork_join_definition(&json!({
            "mode":"each",
            "correlation":{"kind":"root"}
        }));
        assert!(matches!(
            run_to_completion(&each, json!(7)).expect("each join should complete"),
            KernelOutcome::Completed(value) if value == json!([7, 7])
        ));

        let quorum = fork_join_definition(&json!({
            "mode":"quorum",
            "quorum":1,
            "late":"discard",
            "correlation":{"kind":"root"}
        }));
        assert!(matches!(
            run_to_completion(&quorum, json!(7)).expect("quorum join should complete"),
            KernelOutcome::Completed(value) if value == json!({"A":7})
        ));

        let race = fork_join_definition(&json!({
            "mode":"race",
            "late":"discard",
            "correlation":{"kind":"root"}
        }));
        assert!(matches!(
            run_to_completion(&race, json!(7)).expect("race join should complete"),
            KernelOutcome::Completed(value) if value == json!(7)
        ));

        let reduce = fork_join_definition(&json!({
            "mode":"reduce",
            "reducer":{"operation":"sum"},
            "until":{
                "kind":"compare",
                "left":{"kind":"path","root":"output","path":[]},
                "operator":"ge",
                "right":{"kind":"literal","value":14}
            },
            "correlation":{"kind":"root"}
        }));
        assert!(matches!(
            run_to_completion(&reduce, json!(7)).expect("reduce join should complete"),
            KernelOutcome::Completed(value) if value == json!(14.0)
        ));
    }

    #[test]
    fn race_late_arrival_dispositions_discard_cancel_or_error() {
        let cancel = fork_join_definition(&json!({
            "mode":"race",
            "late":"cancel",
            "correlation":{"kind":"root"}
        }));
        assert!(matches!(
            run_to_completion(&cancel, json!(1)).expect("late cancellation is a valid outcome"),
            KernelOutcome::Cancelled
        ));

        let error = fork_join_definition(&json!({
            "mode":"race",
            "late":"error",
            "correlation":{"kind":"root"}
        }));
        let error = run_to_completion(&error, json!(1)).expect_err("late arrival must fail");
        assert!(error.to_string().contains("late arrival at closed join"));
    }

    #[test]
    fn no_match_complete_skip_and_fail_are_not_conflated() {
        let definition_with = |no_match: &str| {
            definition(
                json!({
                    "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                    "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
                }),
                json!({"route":{"id":"route","mode":"firstMatch","noMatch":no_match}}),
                json!({
                    "never":{"id":"never","sourceNodeId":"in","targetNodeId":"out","trigger":"completed","guard":{"kind":"literal","value":false},"priority":0,"routeGroupId":"route"}
                }),
            )
        };
        assert!(matches!(
            run_to_completion(&definition_with("complete"), json!(9))
                .expect("complete policy should terminate"),
            KernelOutcome::Completed(value) if value == json!(9)
        ));
        assert!(matches!(
            run_to_completion(&definition_with("skip"), json!(9))
                .expect("skip policy should terminate"),
            KernelOutcome::Completed(Value::Null)
        ));
        assert!(
            run_to_completion(&definition_with("fail"), json!(9))
                .expect_err("fail policy must reject the run")
                .to_string()
                .contains("matched no transition")
        );
    }

    #[test]
    fn custom_activity_disposition_routes_and_invalid_observation_is_atomic() {
        let workflow = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "work":{
                    "id":"work","label":"Work","scopeId":"root","kind":"activity",
                    "activity":{"kind":"builtin","name":"compress"},
                    "policy":{"authorities":["cap:review"],"maxOutputBytes":32}
                },
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "custom":{"id":"custom","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"work","trigger":"completed","priority":0,"routeGroupId":"start"},
                "custom":{"id":"custom","sourceNodeId":"work","targetNodeId":"out","trigger":"custom:reviewReady","priority":0,"routeGroupId":"custom"}
            }),
        );
        workflow
            .parsed()
            .validate_publishable()
            .expect("custom-disposition workflow should publish");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            workflow.parsed(),
            "run".into(),
            "rho:id:team".into(),
            workflow.definition_hash(),
            json!("request"),
        )
        .expect("continuation should start");
        let KernelOutcome::Effects(requests) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("activity should be requested")
        else {
            panic!("expected an effect")
        };
        let request = requests.first().expect("one effect");
        assert_eq!(request.authorities, ["cap:review"]);

        let oversized = observation(
            request,
            json!("this output is intentionally much larger than 32 bytes"),
            "custom:reviewReady",
            now,
        );
        assert!(
            continuation
                .apply_effect_observation(oversized, now)
                .expect_err("oversized observation must be rejected")
                .to_string()
                .contains("node limit")
        );
        assert!(
            continuation
                .pending_effects
                .contains_key(&request.effect_key)
        );

        continuation
            .apply_effect_observation(
                observation(request, json!("approved"), "custom:reviewReady", now),
                now,
            )
            .expect("valid custom observation should apply");
        assert!(matches!(
            continuation
                .run_until_blocked(workflow.parsed(), now)
                .expect("custom disposition should route"),
            KernelOutcome::Completed(value) if value == json!("approved")
        ));
    }

    #[test]
    fn retry_timeout_and_compensation_form_one_durable_error_policy() {
        let workflow = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "work":{
                    "id":"work","label":"Work","scopeId":"root","kind":"activity",
                    "activity":{"kind":"builtin","name":"compress"},
                    "policy":{
                        "authorities":[],
                        "timeout":{"afterMillis":2500},
                        "retry":{"maxAttempts":2,"initialDelayMillis":1000,"backoffMultiplierMillis":2000,"maxDelayMillis":1000,"deterministicJitter":false,"retryOn":["timedOut"]},
                        "compensation":{"kind":"builtin","name":"compress"}
                    }
                },
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "done":{"id":"done","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"work","trigger":"completed","priority":0,"routeGroupId":"start"},
                "done":{"id":"done","sourceNodeId":"work","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"done"}
            }),
        );
        workflow
            .parsed()
            .validate_publishable()
            .expect("error-policy workflow should publish");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            workflow.parsed(),
            "run".into(),
            "rho:id:team".into(),
            workflow.definition_hash(),
            json!("request"),
        )
        .expect("continuation should start");
        let KernelOutcome::Effects(first_requests) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("first attempt should be requested")
        else {
            panic!("expected first effect")
        };
        let first = first_requests.first().expect("one first attempt");
        assert_eq!(first.attempt, 1);
        assert_eq!(first.timeout_millis, Some(2500));
        continuation
            .apply_effect_observation(
                observation(first, json!({"error":"timeout"}), "timedOut", now),
                now,
            )
            .expect("timeout observation should schedule retry");

        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("retry should suspend durably")
        else {
            panic!("expected retry suspension")
        };
        let retry = waits.first().expect("one retry timer");
        let due_at = retry.due_at.expect("retry must have a deadline");
        continuation
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: retry.id.clone(),
                    expected_deadline: due_at,
                },
                "scheduler",
                due_at,
            )
            .expect("due retry should resume");
        let KernelOutcome::Effects(second_requests) = continuation
            .run_until_blocked(workflow.parsed(), due_at)
            .expect("second attempt should be requested")
        else {
            panic!("expected second effect")
        };
        let second = second_requests.first().expect("one second attempt");
        assert_eq!(second.attempt, 2);
        continuation
            .apply_effect_observation(
                observation(second, json!("done"), "completed", due_at),
                due_at,
            )
            .expect("successful retry should apply");
        assert!(matches!(
            continuation
                .run_until_blocked(workflow.parsed(), due_at)
                .expect("forward workflow should complete"),
            KernelOutcome::Completed(value) if value == json!("done")
        ));

        continuation
            .apply_command(
                ResumeCommand::Cancel {
                    run_id: "run".into(),
                    reason: "customer rollback".into(),
                },
                "owner",
                due_at,
            )
            .expect("cancellation should begin compensation");
        let KernelOutcome::Effects(compensation_requests) = continuation
            .run_until_blocked(workflow.parsed(), due_at)
            .expect("compensation should be requested")
        else {
            panic!("expected compensation effect")
        };
        let compensation = compensation_requests
            .first()
            .expect("one compensation effect");
        assert!(compensation.effect_key.starts_with("compensate:"));
        continuation
            .apply_effect_observation(
                observation(compensation, Value::Null, "completed", due_at),
                due_at,
            )
            .expect("compensation observation should apply");
        assert!(matches!(
            continuation
                .run_until_blocked(workflow.parsed(), due_at)
                .expect("cancelled workflow should terminate"),
            KernelOutcome::Cancelled
        ));
        assert!(
            continuation
                .events
                .iter()
                .any(|event| event.event_type == "effectCompensated")
        );
    }

    #[test]
    fn nested_compound_scopes_propagate_completion_to_the_root() {
        let workflow = WorkflowDefinition::new(json!({
            "schemaVersion":"2.0",
            "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"legacy"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "outer":{"id":"outer","label":"Outer","scopeId":"root","kind":"scope","childScopeId":"outer-scope"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"},
                "outer-in":{"id":"outer-in","label":"Outer input","scopeId":"outer-scope","kind":"input"},
                "inner":{"id":"inner","label":"Inner","scopeId":"outer-scope","kind":"scope","childScopeId":"inner-scope"},
                "outer-final":{"id":"outer-final","label":"Outer final","scopeId":"outer-scope","kind":"final"},
                "inner-in":{"id":"inner-in","label":"Inner input","scopeId":"inner-scope","kind":"input"},
                "inner-final":{"id":"inner-final","label":"Inner final","scopeId":"inner-scope","kind":"final"}
            },
            "policies":{"defaultExecution":"durable","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{
                "to-outer":{"id":"to-outer","mode":"allMatches","noMatch":"fail"},
                "to-out":{"id":"to-out","mode":"allMatches","noMatch":"fail"},
                "to-inner":{"id":"to-inner","mode":"allMatches","noMatch":"fail"},
                "outer-complete":{"id":"outer-complete","mode":"allMatches","noMatch":"fail"},
                "inner-complete":{"id":"inner-complete","mode":"allMatches","noMatch":"fail"}
            },
            "scopes":{
                "root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"},
                "outer-scope":{"id":"outer-scope","label":"Outer","kind":"compound","parentScopeId":"root","initialNodeIds":["outer-in"],"entryActivities":[],"exitActivities":[],"history":"none"},
                "inner-scope":{"id":"inner-scope","label":"Inner","kind":"compound","parentScopeId":"outer-scope","initialNodeIds":["inner-in"],"entryActivities":[],"exitActivities":[],"history":"none"}
            },
            "transitions":{
                "in-outer":{"id":"in-outer","sourceNodeId":"in","targetNodeId":"outer","trigger":"completed","priority":0,"routeGroupId":"to-outer"},
                "outer-out":{"id":"outer-out","sourceNodeId":"outer","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"to-out"},
                "outer-in-inner":{"id":"outer-in-inner","sourceNodeId":"outer-in","targetNodeId":"inner","trigger":"completed","priority":0,"routeGroupId":"to-inner"},
                "inner-outer-final":{"id":"inner-outer-final","sourceNodeId":"inner","targetNodeId":"outer-final","trigger":"completed","priority":0,"routeGroupId":"outer-complete"},
                "inner-in-final":{"id":"inner-in-final","sourceNodeId":"inner-in","targetNodeId":"inner-final","trigger":"completed","priority":0,"routeGroupId":"inner-complete"}
            },
            "types":{}
        }))
        .expect("nested scope workflow should parse");

        assert!(matches!(
            run_to_completion(&workflow, json!({"request":1}))
                .expect("nested scopes should complete"),
            KernelOutcome::Completed(value) if value == json!({"request":1})
        ));
    }

    #[test]
    fn scope_entry_exit_lifecycle_and_deep_history_are_executable() {
        let workflow = WorkflowDefinition::new(json!({
            "schemaVersion":"2.0",
            "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"legacy"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "scope":{"id":"scope","label":"Remembering scope","scopeId":"root","kind":"scope","childScopeId":"child"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"},
                "entry":{"id":"entry","label":"Entry effect","scopeId":"child","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "first":{"id":"first","label":"First state","scopeId":"child","kind":"gateway"},
                "remembered":{"id":"remembered","label":"Remembered state","scopeId":"child","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "exit":{"id":"exit","label":"Exit effect","scopeId":"child","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "final":{"id":"final","label":"Final","scopeId":"child","kind":"final"}
            },
            "policies":{"defaultExecution":"durable","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "again":{"id":"again","mode":"allMatches","noMatch":"fail"},
                "limit":{"id":"limit","mode":"allMatches","noMatch":"fail"},
                "first":{"id":"first","mode":"allMatches","noMatch":"fail"},
                "finish":{"id":"finish","mode":"allMatches","noMatch":"fail"}
            },
            "scopes":{
                "root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"},
                "child":{"id":"child","label":"Child","kind":"compound","parentScopeId":"root","initialNodeIds":["first"],"entryActivities":["entry"],"exitActivities":["exit"],"history":"deep"}
            },
            "transitions":{
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"scope","trigger":"completed","priority":0,"routeGroupId":"start"},
                "again":{"id":"again","sourceNodeId":"scope","targetNodeId":"scope","trigger":"completed","priority":0,"routeGroupId":"again","loop":{"maxIterations":1}},
                "limit":{"id":"limit","sourceNodeId":"scope","targetNodeId":"out","trigger":"iterationLimit","priority":0,"routeGroupId":"limit"},
                "first":{"id":"first","sourceNodeId":"first","targetNodeId":"remembered","trigger":"completed","priority":0,"routeGroupId":"first"},
                "finish":{"id":"finish","sourceNodeId":"remembered","targetNodeId":"final","trigger":"completed","priority":0,"routeGroupId":"finish"}
            },
            "types":{}
        }))
        .expect("history workflow should parse");
        workflow
            .parsed()
            .validate_publishable()
            .expect("history workflow should publish");
        let mut continuation = ContinuationEnvelope::start(
            workflow.parsed(),
            "run".into(),
            "rho:id:team".into(),
            workflow.definition_hash(),
            json!("input"),
        )
        .expect("history continuation should start");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut effect_nodes = Vec::new();
        loop {
            match continuation
                .run_until_blocked(workflow.parsed(), now)
                .expect("history workflow kernel should progress")
            {
                KernelOutcome::Effects(requests) => {
                    let request = requests.first().expect("one serial scope effect");
                    effect_nodes.push(request.node_id.clone());
                    continuation
                        .apply_effect_observation(
                            observation(request, request.input.clone(), "completed", now),
                            now,
                        )
                        .expect("scope effect should complete");
                }
                KernelOutcome::Completed(value) => {
                    assert_eq!(value, json!("input"));
                    break;
                }
                other => panic!("unexpected hierarchy outcome: {other:?}"),
            }
        }

        assert_eq!(
            effect_nodes,
            ["entry", "remembered", "exit", "entry", "remembered", "exit"]
        );
        assert_eq!(
            continuation
                .events
                .iter()
                .filter(|event| {
                    event.event_type == "nodeEntered" && event.node_id.as_deref() == Some("first")
                })
                .count(),
            1,
            "the second scope entry must restore the remembered state"
        );
        assert!(continuation.scopes.is_empty());
    }

    #[test]
    fn timer_wait_requires_the_exact_due_command() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "wait":{"id":"wait","label":"Wait","scopeId":"root","kind":"wait","waitFor":"timer","timer":{"delayMillis":1000}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "resume":{"id":"resume","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"wait","trigger":"completed","priority":0,"routeGroupId":"start"},
                "resume":{"id":"resume","sourceNodeId":"wait","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"resume"}
            }),
        );
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!("value"),
        )
        .expect("continuation should start");
        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(definition.parsed(), now)
            .expect("kernel should suspend")
        else {
            panic!("expected suspension")
        };
        let wait = waits.first().expect("one wait");
        let due = wait.due_at.expect("timer deadline");
        assert!(
            continuation
                .apply_command(
                    ResumeCommand::TimerFired {
                        run_id: "run".into(),
                        wait_id: wait.id.clone(),
                        expected_deadline: due,
                    },
                    "scheduler",
                    now,
                )
                .is_err()
        );
        continuation
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    expected_deadline: due,
                },
                "scheduler",
                due,
            )
            .expect("due timer should resume");
        assert!(matches!(
            continuation
                .run_until_blocked(definition.parsed(), due)
                .expect("kernel should complete"),
            KernelOutcome::Completed(Value::String(value)) if value == "value"
        ));
    }

    #[test]
    fn approval_quorum_is_authorized_idempotent_and_rejectable() {
        let workflow = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "approval":{"id":"approval","label":"Review","scopeId":"root","kind":"approval","policyId":"review-policy","quorum":2,"approvers":["alice","bob"]},
                "approved":{"id":"approved","label":"Approved","scopeId":"root","kind":"output"},
                "rejected":{"id":"rejected","label":"Rejected","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "approved":{"id":"approved","mode":"allMatches","noMatch":"fail"},
                "rejected":{"id":"rejected","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"approval","trigger":"completed","priority":0,"routeGroupId":"start"},
                "approved":{"id":"approved","sourceNodeId":"approval","targetNodeId":"approved","trigger":"completed","priority":0,"routeGroupId":"approved"},
                "rejected":{"id":"rejected","sourceNodeId":"approval","targetNodeId":"rejected","trigger":"rejected","priority":0,"routeGroupId":"rejected"}
            }),
        );
        workflow
            .parsed()
            .validate_publishable()
            .expect("approval workflow should publish");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let start = || {
            ContinuationEnvelope::start(
                workflow.parsed(),
                "run".into(),
                "rho:id:team".into(),
                workflow.definition_hash(),
                json!("request"),
            )
            .expect("approval continuation should start")
        };

        let mut approved = start();
        let KernelOutcome::Suspended(waits) = approved
            .run_until_blocked(workflow.parsed(), now)
            .expect("approval should suspend")
        else {
            panic!("expected approval suspension")
        };
        let wait_id = waits.first().expect("one approval wait").id.clone();
        let approve = |actor: &str, decision_id: &str| {
            (
                ResumeCommand::Approve {
                    run_id: "run".into(),
                    wait_id: wait_id.clone(),
                    decision_id: decision_id.into(),
                    payload: json!({"approvedBy":actor}),
                },
                actor.to_owned(),
            )
        };

        let (command, actor) = approve("mallory", "unauthorized");
        assert!(
            approved
                .apply_command(command, &actor, now)
                .expect_err("an unlisted approver must be rejected")
                .to_string()
                .contains("not an approver")
        );
        assert!(approved.pending_waits.contains_key(&wait_id));

        let (command, actor) = approve("alice", "decision-1");
        approved
            .apply_command(command, &actor, now)
            .expect("the first approval should be recorded");
        let approval = approved
            .pending_waits
            .get(&wait_id)
            .and_then(|wait| wait.approval.as_ref())
            .expect("the approval must remain pending below quorum");
        assert_eq!(
            approval.decisions.get("alice").map(String::as_str),
            Some("decision-1")
        );
        assert_eq!(
            approved
                .events
                .iter()
                .filter(|event| event.event_type == "runResumed")
                .count(),
            0,
            "a partial quorum vote must not claim that the run resumed"
        );

        let (command, actor) = approve("alice", "decision-2");
        assert!(
            approved
                .apply_command(command, &actor, now)
                .expect_err("one approver may decide only once")
                .to_string()
                .contains("already decided")
        );
        let (command, actor) = approve("bob", "decision-1");
        assert!(
            approved
                .apply_command(command, &actor, now)
                .expect_err("decision identities must be globally unique")
                .to_string()
                .contains("decision already recorded")
        );
        assert!(approved.pending_waits.contains_key(&wait_id));

        let (command, actor) = approve("bob", "decision-2");
        approved
            .apply_command(command, &actor, now)
            .expect("the second distinct approver should satisfy quorum");
        assert!(!approved.pending_waits.contains_key(&wait_id));
        assert_eq!(
            approved
                .events
                .iter()
                .filter(|event| event.event_type == "approvalRecorded")
                .count(),
            2
        );
        assert!(matches!(
            approved
                .run_until_blocked(workflow.parsed(), now)
                .expect("quorum should route through completed"),
            KernelOutcome::Completed(value) if value == json!({"approvedBy":"bob"})
        ));

        let mut rejected = start();
        let KernelOutcome::Suspended(waits) = rejected
            .run_until_blocked(workflow.parsed(), now)
            .expect("approval should suspend")
        else {
            panic!("expected approval suspension")
        };
        let wait_id = waits.first().expect("one approval wait").id.clone();
        assert!(
            rejected
                .apply_command(
                    ResumeCommand::Reject {
                        run_id: "run".into(),
                        wait_id: wait_id.clone(),
                        payload: json!("denied"),
                    },
                    "mallory",
                    now,
                )
                .expect_err("an unlisted rejector must be rejected")
                .to_string()
                .contains("not an approver")
        );
        assert!(rejected.pending_waits.contains_key(&wait_id));
        rejected
            .apply_command(
                ResumeCommand::Reject {
                    run_id: "run".into(),
                    wait_id,
                    payload: json!("denied"),
                },
                "alice",
                now,
            )
            .expect("an authorized rejection should resume the rejected route");
        assert!(matches!(
            rejected
                .run_until_blocked(workflow.parsed(), now)
                .expect("rejection should route through rejected"),
            KernelOutcome::Completed(value) if value == json!("denied")
        ));
    }

    // The Research Desk demo's gate tail: an owner-quorum-1 approval whose
    // completed route feeds an activity (the Archivist) before the output.
    // Approval-into-activity is otherwise uncovered: every other approval test
    // routes the gate straight to output nodes.
    fn gate_archive_definition() -> WorkflowDefinition {
        definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "gate":{"id":"gate","label":"Gate","scopeId":"root","kind":"approval","policyId":"owner","quorum":1,"approvers":[]},
                "archive":{"id":"archive","label":"Archive","scopeId":"root","kind":"activity",
                    "policy":{"retry":{"maxAttempts":2,"initialDelayMillis":1000,"backoffMultiplierMillis":1000,"maxDelayMillis":1000,"deterministicJitter":false,"retryOn":[]}},
                    "activity":{"kind":"agent","agentId":"archivist","agentVersion":"v1","protocol":"structured-v1"}},
                "out":{"id":"out","label":"Answer","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "gate-completed":{"id":"gate-completed","mode":"allMatches","noMatch":"fail"},
                "archive-completed":{"id":"archive-completed","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"gate","trigger":"completed","priority":0,"routeGroupId":"start"},
                "approved":{"id":"approved","sourceNodeId":"gate","targetNodeId":"archive","trigger":"completed","priority":0,"routeGroupId":"gate-completed"},
                "archive-out":{"id":"archive-out","sourceNodeId":"archive","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"archive-completed"}
            }),
        )
    }

    fn suspend_and_approve(
        workflow: &WorkflowDefinition,
        payload: Value,
        now: DateTime<Utc>,
    ) -> ContinuationEnvelope {
        let mut continuation = ContinuationEnvelope::start(
            workflow.parsed(),
            "run".into(),
            "rho:id:team".into(),
            workflow.definition_hash(),
            json!("request"),
        )
        .expect("gate continuation should start");
        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("the gate should suspend")
        else {
            panic!("expected gate suspension")
        };
        let wait_id = waits.first().expect("one gate wait").id.clone();
        continuation
            .apply_command(
                ResumeCommand::Approve {
                    run_id: "run".into(),
                    wait_id,
                    decision_id: "decision-1".into(),
                    payload,
                },
                "owner",
                now,
            )
            .expect("an owner approval should be recorded");
        continuation
    }

    #[test]
    fn approval_payload_replacement_flows_through_downstream_activity_to_output() {
        let workflow = gate_archive_definition();
        workflow
            .parsed()
            .validate_publishable()
            .expect("gate-archive workflow should publish");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let replacement = json!({"question":"q?","answer":"human","feedback":"human","score":100});
        let mut continuation = suspend_and_approve(&workflow, replacement.clone(), now);

        let KernelOutcome::Effects(requests) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("the approved token should reach the activity")
        else {
            panic!("expected the archive invocation")
        };
        let request = requests.first().expect("one archive invocation");
        assert_eq!(requests.len(), 1);
        assert_eq!(request.node_id, "archive");
        assert_eq!(request.attempt, 1);
        assert_eq!(
            request.input, replacement,
            "the approve payload must replace the released value before the activity runs"
        );

        let archivist_output = json!({
            "question":"q?","answer":"human","feedback":"human","score":100,
            "archived":true
        });
        continuation
            .apply_effect_observation(
                observation(request, archivist_output.clone(), "completed", now),
                now,
            )
            .expect("the archive observation should apply");
        assert!(matches!(
            continuation
                .run_until_blocked(workflow.parsed(), now)
                .expect("the archived record should reach the output"),
            KernelOutcome::Completed(value) if value == archivist_output
        ));
    }

    #[test]
    fn downstream_activity_failure_after_approval_schedules_retry_then_completes() {
        let workflow = gate_archive_definition();
        workflow
            .parsed()
            .validate_publishable()
            .expect("gate-archive workflow should publish");
        let now = DateTime::<Utc>::UNIX_EPOCH;

        // Null payload: the token value waiting at the gate flows on unchanged.
        let mut continuation = suspend_and_approve(&workflow, Value::Null, now);
        let KernelOutcome::Effects(requests) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("the approved token should reach the activity")
        else {
            panic!("expected the archive invocation")
        };
        let first = requests.first().expect("one archive invocation");
        assert_eq!(
            first.input,
            json!("request"),
            "a null approve payload must preserve the released value"
        );
        continuation
            .apply_effect_observation(
                observation(
                    first,
                    json!({"error":"activity deploy errored on chain"}),
                    "failed",
                    now,
                ),
                now,
            )
            .expect("the failed observation should schedule a retry");

        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(workflow.parsed(), now)
            .expect("the retry should suspend durably")
        else {
            panic!("expected a retry suspension")
        };
        let retry = waits.first().expect("one retry wait");
        assert!(
            retry.id.starts_with("retry:") && retry.id.ends_with(":2"),
            "the retry wait must carry the bumped attempt: {}",
            retry.id
        );
        let due_at = retry.due_at.expect("the retry must have a deadline");
        assert_eq!(due_at, now + chrono::Duration::milliseconds(1000));
        assert!(
            continuation
                .events
                .iter()
                .any(|event| event.event_type == "retryScheduled"),
            "the retry must be announced on the public event trail"
        );

        continuation
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: retry.id.clone(),
                    expected_deadline: due_at,
                },
                "scheduler",
                due_at,
            )
            .expect("the due retry should resume");
        let KernelOutcome::Effects(requests) = continuation
            .run_until_blocked(workflow.parsed(), due_at)
            .expect("the retry should re-invoke the activity")
        else {
            panic!("expected the second archive invocation")
        };
        let second = requests.first().expect("one retried invocation");
        assert_eq!(second.attempt, 2);
        assert_ne!(
            second.effect_key, first.effect_key,
            "the retried invocation must mint a fresh effect key"
        );
        continuation
            .apply_effect_observation(
                observation(second, json!("request"), "completed", due_at),
                due_at,
            )
            .expect("the retried observation should apply");
        assert!(matches!(
            continuation
                .run_until_blocked(workflow.parsed(), due_at)
                .expect("the retried token should reach the output"),
            KernelOutcome::Completed(value) if value == json!("request")
        ));

        // Exhaustion: with maxAttempts 2, a second failure leaves the token
        // with disposition failed, and the archive node's completed-only
        // routing fails the run honestly instead of emitting an error record
        // as the answer.
        let mut exhausted = suspend_and_approve(&workflow, Value::Null, now);
        let KernelOutcome::Effects(requests) = exhausted
            .run_until_blocked(workflow.parsed(), now)
            .expect("the approved token should reach the activity")
        else {
            panic!("expected the archive invocation")
        };
        exhausted
            .apply_effect_observation(
                observation(
                    requests.first().expect("one archive invocation"),
                    json!({"error":"chroma unreachable"}),
                    "failed",
                    now,
                ),
                now,
            )
            .expect("the failed observation should schedule a retry");
        let KernelOutcome::Suspended(waits) = exhausted
            .run_until_blocked(workflow.parsed(), now)
            .expect("the retry should suspend durably")
        else {
            panic!("expected a retry suspension")
        };
        let retry = waits.first().expect("one retry wait");
        let due_at = retry.due_at.expect("the retry must have a deadline");
        exhausted
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: retry.id.clone(),
                    expected_deadline: due_at,
                },
                "scheduler",
                due_at,
            )
            .expect("the due retry should resume");
        let KernelOutcome::Effects(requests) = exhausted
            .run_until_blocked(workflow.parsed(), due_at)
            .expect("the retry should re-invoke the activity")
        else {
            panic!("expected the second archive invocation")
        };
        exhausted
            .apply_effect_observation(
                observation(
                    requests.first().expect("one retried invocation"),
                    json!({"error":"chroma unreachable"}),
                    "failed",
                    due_at,
                ),
                due_at,
            )
            .expect("the exhausted observation itself applies; routing fails next");
        assert!(
            exhausted
                .run_until_blocked(workflow.parsed(), due_at)
                .expect_err("exhausted retries must fail the run, not fake an answer")
                .to_string()
                .contains("produced disposition failed with no matching route group")
        );
    }

    #[test]
    fn path_qualified_command_resumes_the_exact_nested_wait() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "wait":{"id":"wait","label":"Wait","scopeId":"root","kind":"wait","waitFor":"timer","timer":{"delayMillis":1000}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "resume":{"id":"resume","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"wait","trigger":"completed","priority":0,"routeGroupId":"start"},
                "resume":{"id":"resume","sourceNodeId":"wait","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"resume"}
            }),
        );
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut child = ContinuationEnvelope::start_nested(
            definition.parsed(),
            "run".into(),
            "child-execution".into(),
            "rho:id:child".into(),
            definition.definition_hash(),
            json!("nested value"),
        )
        .expect("nested continuation should start");
        let KernelOutcome::Suspended(waits) = child
            .run_until_blocked(definition.parsed(), now)
            .expect("nested workflow should suspend")
        else {
            panic!("expected nested suspension")
        };
        let nested_wait = waits.first().expect("one nested wait");
        let due = nested_wait.due_at.expect("nested timer deadline");

        let mut parent = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:parent".into(),
            definition.definition_hash(),
            Value::Null,
        )
        .expect("parent continuation should start");
        parent
            .nested_workflows
            .insert("child-execution".into(), Box::new(child));
        parent
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: format!("nested:child-execution:{}", nested_wait.id),
                    expected_deadline: due,
                },
                "scheduler",
                due,
            )
            .expect("qualified timer command should reach the child");

        let child = parent
            .nested_workflows
            .get_mut("child-execution")
            .expect("child should remain active until observed by its parent");
        assert!(child.pending_waits.is_empty());
        assert!(matches!(
            child
                .run_until_blocked(definition.parsed(), due)
                .expect("resumed child should complete"),
            KernelOutcome::Completed(Value::String(value)) if value == "nested value"
        ));
    }

    #[test]
    fn nested_timer_discovery_returns_a_recursively_qualified_wait() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "wait":{"id":"wait","label":"Wait","scopeId":"root","kind":"wait","waitFor":"timer","timer":{"delayMillis":1000}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "resume":{"id":"resume","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"wait","trigger":"completed","priority":0,"routeGroupId":"start"},
                "resume":{"id":"resume","sourceNodeId":"wait","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"resume"}
            }),
        );
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut grandchild = ContinuationEnvelope::start_nested(
            definition.parsed(),
            "run".into(),
            "grandchild-execution".into(),
            "rho:id:grandchild".into(),
            definition.definition_hash(),
            Value::Null,
        )
        .expect("grandchild should start");
        let KernelOutcome::Suspended(waits) = grandchild
            .run_until_blocked(definition.parsed(), now)
            .expect("grandchild timer should suspend")
        else {
            panic!("expected grandchild suspension")
        };
        let wait = waits.first().expect("one timer wait");
        let due_at = wait.due_at.expect("timer deadline");

        let mut child = ContinuationEnvelope::start_nested(
            definition.parsed(),
            "run".into(),
            "child-execution".into(),
            "rho:id:child".into(),
            definition.definition_hash(),
            Value::Null,
        )
        .expect("child should start");
        child
            .nested_workflows
            .insert("grandchild-execution".into(), Box::new(grandchild));
        let mut root = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:root".into(),
            definition.definition_hash(),
            Value::Null,
        )
        .expect("root should start");
        root.nested_workflows
            .insert("child-execution".into(), Box::new(child));

        assert!(
            root.earliest_due_timer(due_at - chrono::Duration::milliseconds(1))
                .is_none()
        );
        assert_eq!(
            root.earliest_due_timer(due_at),
            Some((
                format!(
                    "nested:child-execution:nested:grandchild-execution:{}",
                    wait.id
                ),
                due_at,
            ))
        );
    }

    #[test]
    fn durable_yield_parks_with_due_now_and_resumes_via_timer_fired_in_enter_phase() {
        let definition = definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "act":{"id":"act","label":"Act","scopeId":"root","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "route":{"id":"route","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"act","trigger":"completed","priority":0,"routeGroupId":"start"},
                "again":{"id":"again","sourceNodeId":"act","targetNodeId":"act","trigger":"completed","priority":0,"routeGroupId":"route",
                         "loop":{"durableYield":true},
                         "guard":{"kind":"compare","left":{"kind":"path","root":"output","path":["counter"]},"operator":"lt","right":{"kind":"literal","value":3}}},
                "done":{"id":"done","sourceNodeId":"act","targetNodeId":"out","trigger":"completed","priority":1,"routeGroupId":"route",
                        "guard":{"kind":"compare","left":{"kind":"path","root":"output","path":["counter"]},"operator":"ge","right":{"kind":"literal","value":3}}}
            }),
        );
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!({"topic":"loop"}),
        )
        .expect("continuation should start");

        let mut counter = 0i64;
        loop {
            let KernelOutcome::Effects(requests) = continuation
                .run_until_blocked(definition.parsed(), now)
                .expect("kernel should request the activity")
            else {
                panic!("expected an activity effect request")
            };
            let request = requests.first().expect("one effect request");
            assert_eq!(
                request.node_id, "act",
                "every iteration re-enters the target activity"
            );
            counter += 1;
            continuation
                .apply_effect_observation(
                    observation(request, json!({"counter": counter}), "completed", now),
                    now,
                )
                .expect("observation should apply");
            if counter >= 3 {
                break;
            }

            let KernelOutcome::Suspended(waits) = continuation
                .run_until_blocked(definition.parsed(), now)
                .expect("kernel should park the durable yield")
            else {
                panic!("expected a durable-yield suspension")
            };
            let wait = waits.first().expect("one yield wait");
            assert!(wait.id.starts_with("yield:"), "wait id is {}", wait.id);
            assert!(matches!(wait.kind, SuspensionKind::DurableYield));
            let due = wait.due_at.expect("durable yields now carry a deadline");
            assert_eq!(due, now, "the yield deadline is the park instant");
            assert_eq!(
                continuation.earliest_due_timer(now),
                Some((wait.id.clone(), due)),
                "the scheduler search finds the yield"
            );

            let mismatched = continuation.apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    expected_deadline: due + chrono::Duration::seconds(1),
                },
                "scheduler",
                due,
            );
            assert!(
                mismatched
                    .expect_err("mismatched deadline must be rejected")
                    .to_string()
                    .contains("timer deadline mismatch")
            );
            let early = continuation.apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    expected_deadline: due,
                },
                "scheduler",
                due - chrono::Duration::milliseconds(1),
            );
            assert!(
                early
                    .expect_err("early fire must be rejected")
                    .to_string()
                    .contains("timer fired before its deadline")
            );
            continuation
                .apply_command(
                    ResumeCommand::TimerFired {
                        run_id: "run".into(),
                        wait_id: wait.id.clone(),
                        expected_deadline: due,
                    },
                    "scheduler",
                    due,
                )
                .expect("the due yield should resume");
            let tail = continuation
                .events
                .iter()
                .rev()
                .take(2)
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>();
            assert_eq!(tail, ["runResumed", "waitSatisfied"]);
        }

        assert!(matches!(
            continuation
                .run_until_blocked(definition.parsed(), now)
                .expect("kernel should complete"),
            KernelOutcome::Completed(value) if value == json!({"counter": 3})
        ));
        let scheduler_events = continuation
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.event_type.as_str(),
                    "durableYield" | "waitSatisfied" | "runResumed"
                )
            })
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            scheduler_events,
            [
                "durableYield",
                "waitSatisfied",
                "runResumed",
                "durableYield",
                "waitSatisfied",
                "runResumed"
            ],
            "each iteration parks and resumes exactly once"
        );
    }

    fn approval_deadline_definition() -> WorkflowDefinition {
        definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "gate":{"id":"gate","label":"Gate","scopeId":"root","kind":"approval","policyId":"owner","quorum":1,"approvers":["alice"],
                        "policy":{"timeout":{"afterMillis":1000}}},
                "out-ok":{"id":"out-ok","label":"Approved","scopeId":"root","kind":"output"},
                "out-rej":{"id":"out-rej","label":"Rejected","scopeId":"root","kind":"output"},
                "out-late":{"id":"out-late","label":"TimedOut","scopeId":"root","kind":"output"}
            }),
            json!({
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "ok":{"id":"ok","mode":"allMatches","noMatch":"fail"},
                "rej":{"id":"rej","mode":"allMatches","noMatch":"fail"},
                "late":{"id":"late","mode":"allMatches","noMatch":"fail"}
            }),
            json!({
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"gate","trigger":"completed","priority":0,"routeGroupId":"start"},
                "ok":{"id":"ok","sourceNodeId":"gate","targetNodeId":"out-ok","trigger":"completed","priority":0,"routeGroupId":"ok"},
                "rej":{"id":"rej","sourceNodeId":"gate","targetNodeId":"out-rej","trigger":"rejected","priority":0,"routeGroupId":"rej"},
                "late":{"id":"late","sourceNodeId":"gate","targetNodeId":"out-late","trigger":"timedOut","priority":0,"routeGroupId":"late"}
            }),
        )
    }

    #[test]
    fn approval_deadline_expiry_resumes_timed_out_and_rejects_late_decisions() {
        let definition = approval_deadline_definition();
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!("value"),
        )
        .expect("continuation should start");
        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(definition.parsed(), now)
            .expect("kernel should suspend")
        else {
            panic!("expected the approval suspension")
        };
        let wait = waits.first().expect("one approval wait");
        assert!(matches!(wait.kind, SuspensionKind::Approval));
        let due = wait.due_at.expect("the timeout policy sets a deadline");
        assert_eq!(due, now + chrono::Duration::milliseconds(1000));
        assert_eq!(
            continuation.earliest_due_timer(due),
            Some((wait.id.clone(), due)),
            "the scheduler search finds the approval deadline"
        );

        continuation
            .apply_command(
                ResumeCommand::TimerFired {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    expected_deadline: due,
                },
                "scheduler",
                due,
            )
            .expect("the expired approval should time out");
        assert!(
            continuation
                .events
                .iter()
                .all(|event| event.event_type != "approvalRecorded"),
            "a timeout is not an approval decision"
        );
        assert!(matches!(
            continuation
                .run_until_blocked(definition.parsed(), due)
                .expect("kernel should complete via the timedOut route"),
            KernelOutcome::Completed(_)
        ));
        assert!(
            continuation
                .events
                .iter()
                .any(|event| event.event_type == "nodeEntered"
                    && event.node_id.as_deref() == Some("out-late")),
            "the timedOut trigger routes to the late branch"
        );

        let late_approve = continuation.apply_command(
            ResumeCommand::Approve {
                run_id: "run".into(),
                wait_id: wait.id.clone(),
                decision_id: "d1".into(),
                payload: Value::Null,
            },
            "alice",
            due,
        );
        assert!(
            late_approve
                .expect_err("a decision after expiry must fail")
                .to_string()
                .contains("is not pending")
        );
        let late_reject = continuation.apply_command(
            ResumeCommand::Reject {
                run_id: "run".into(),
                wait_id: wait.id.clone(),
                payload: Value::Null,
            },
            "alice",
            due,
        );
        assert!(
            late_reject
                .expect_err("a rejection after expiry must fail")
                .to_string()
                .contains("is not pending")
        );
    }

    #[test]
    fn approval_quorum_before_deadline_still_completes_and_late_timer_fire_is_rejected() {
        let definition = approval_deadline_definition();
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!("value"),
        )
        .expect("continuation should start");
        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(definition.parsed(), now)
            .expect("kernel should suspend")
        else {
            panic!("expected the approval suspension")
        };
        let wait = waits.first().expect("one approval wait");
        let due = wait.due_at.expect("the timeout policy sets a deadline");

        continuation
            .apply_command(
                ResumeCommand::Approve {
                    run_id: "run".into(),
                    wait_id: wait.id.clone(),
                    decision_id: "d1".into(),
                    payload: Value::Null,
                },
                "alice",
                now,
            )
            .expect("the quorum decision should resume the run");
        assert!(
            continuation
                .events
                .iter()
                .any(|event| event.event_type == "approvalRecorded")
        );
        assert!(matches!(
            continuation
                .run_until_blocked(definition.parsed(), now)
                .expect("kernel should complete via the approved route"),
            KernelOutcome::Completed(_)
        ));
        assert!(
            continuation
                .events
                .iter()
                .any(|event| event.event_type == "nodeEntered"
                    && event.node_id.as_deref() == Some("out-ok"))
        );

        let stale_fire = continuation.apply_command(
            ResumeCommand::TimerFired {
                run_id: "run".into(),
                wait_id: wait.id.clone(),
                expected_deadline: due,
            },
            "scheduler",
            due,
        );
        assert!(
            stale_fire
                .expect_err("the resolved approval's deadline must not fire")
                .to_string()
                .contains("is not pending")
        );
    }

    #[test]
    fn history_node_multi_target_restore_preserves_scope_accounting() {
        let document = json!({
            "schemaVersion":"2.0",
            "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"legacy"}},"outputs":{"output":{"kind":"legacy"}}},
            "policies":{"defaultExecution":"durable","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "nodes":{
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "review":{"id":"review","label":"Review scope","scopeId":"root","kind":"scope","childScopeId":"child"},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"},
                "gate":{"id":"gate","label":"Gate","scopeId":"child","kind":"gateway"},
                "act-a":{"id":"act-a","label":"Act A","scopeId":"child","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "act-b":{"id":"act-b","label":"Act B","scopeId":"child","kind":"activity","activity":{"kind":"builtin","name":"compress"}},
                "h":{"id":"h","label":"History","scopeId":"child","kind":"history","history":"deep"}
            },
            "routeGroups":{
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "again":{"id":"again","mode":"allMatches","noMatch":"fail"},
                "limit":{"id":"limit","mode":"allMatches","noMatch":"fail"},
                "fan":{"id":"fan","mode":"allMatches","noMatch":"fail"},
                "a-done":{"id":"a-done","mode":"allMatches","noMatch":"skip"},
                "a-failed":{"id":"a-failed","mode":"allMatches","noMatch":"fail"},
                "b-done":{"id":"b-done","mode":"allMatches","noMatch":"skip"}
            },
            "scopes":{
                "root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"},
                "child":{"id":"child","label":"Child","kind":"parallel","parentScopeId":"root","initialNodeIds":["gate"],"entryActivities":[],"exitActivities":[],"history":"deep"}
            },
            "transitions":{
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"review","trigger":"completed","priority":0,"routeGroupId":"start"},
                "again":{"id":"again","sourceNodeId":"review","targetNodeId":"review","trigger":"completed","priority":0,"routeGroupId":"again","loop":{"maxIterations":1}},
                "limit":{"id":"limit","sourceNodeId":"review","targetNodeId":"out","trigger":"iterationLimit","priority":0,"routeGroupId":"limit"},
                "fan-a":{"id":"fan-a","sourceNodeId":"gate","targetNodeId":"act-a","trigger":"completed","priority":0,"routeGroupId":"fan"},
                "fan-b":{"id":"fan-b","sourceNodeId":"gate","targetNodeId":"act-b","trigger":"completed","priority":1,"routeGroupId":"fan"},
                "a-done":{"id":"a-done","sourceNodeId":"act-a","targetNodeId":"h","trigger":"completed","priority":0,"routeGroupId":"a-done",
                          "guard":{"kind":"literal","value":false}},
                "a-failed":{"id":"a-failed","sourceNodeId":"act-a","targetNodeId":"h","trigger":"failed","priority":0,"routeGroupId":"a-failed"},
                "b-done":{"id":"b-done","sourceNodeId":"act-b","targetNodeId":"h","trigger":"completed","priority":0,"routeGroupId":"b-done",
                          "guard":{"kind":"literal","value":false}}
            },
            "types":{}
        });
        let definition = WorkflowDefinition::new(document).expect("definition should parse");
        definition
            .parsed()
            .validate_publishable()
            .expect("definition should be publishable");
        let now = DateTime::<Utc>::UNIX_EPOCH;
        let mut continuation = ContinuationEnvelope::start(
            definition.parsed(),
            "run".into(),
            "rho:id:team".into(),
            definition.definition_hash(),
            json!("value"),
        )
        .expect("continuation should start");

        let observe_effects = |continuation: &mut ContinuationEnvelope,
                               definition_ref: &super::super::workflow::Definition,
                               dispositions: &[(&str, &str)]| {
            let KernelOutcome::Effects(requests) = continuation
                .run_until_blocked(definition_ref, now)
                .expect("kernel should request the child activities")
            else {
                panic!("expected activity effect requests")
            };
            assert_eq!(requests.len(), dispositions.len());
            for (node, disposition) in dispositions {
                let request = requests
                    .iter()
                    .find(|request| request.node_id == *node)
                    .unwrap_or_else(|| panic!("no pending effect for {node}"));
                continuation
                    .apply_effect_observation(
                        observation(request, json!({"from": node}), disposition, now),
                        now,
                    )
                    .expect("observation should apply");
            }
        };

        observe_effects(
            &mut continuation,
            definition.parsed(),
            &[("act-a", "completed"), ("act-b", "completed")],
        );
        assert_eq!(
            continuation.history.len(),
            0,
            "history is captured only at completion"
        );

        observe_effects(
            &mut continuation,
            definition.parsed(),
            &[("act-b", "completed"), ("act-a", "failed")],
        );

        let KernelOutcome::Effects(restored) = continuation
            .run_until_blocked(definition.parsed(), now)
            .expect("the history node should restore both remembered targets")
        else {
            panic!("expected the restored activity effects")
        };
        assert_eq!(
            restored.len(),
            2,
            "the deep history node restores both remembered nodes"
        );
        let instance = continuation
            .scopes
            .values()
            .next()
            .expect("the child scope instance is live");
        assert_eq!(
            instance.active_tokens, 2,
            "restoration adjusts the live-token count for every emitted target"
        );

        for (node, disposition) in [("act-a", "completed"), ("act-b", "completed")] {
            let request = restored
                .iter()
                .find(|request| request.node_id == node)
                .unwrap_or_else(|| panic!("no restored effect for {node}"));
            continuation
                .apply_effect_observation(
                    observation(request, json!({"from": node}), disposition, now),
                    now,
                )
                .expect("observation should apply");
        }
        assert!(matches!(
            continuation
                .run_until_blocked(definition.parsed(), now)
                .expect("the scope must complete exactly once, with no lost instance"),
            KernelOutcome::Completed(Value::String(value)) if value == "value"
        ));
        assert!(continuation.scopes.is_empty(), "no instance leaks");
    }

    fn path_deadline_definition(with_failed_route: bool) -> WorkflowDefinition {
        let mut transitions = json!({
            "start":{"id":"start","sourceNodeId":"in","targetNodeId":"wait","trigger":"completed","priority":0,"routeGroupId":"start"},
            "done":{"id":"done","sourceNodeId":"wait","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"done"}
        });
        let mut groups = json!({
            "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
            "done":{"id":"done","mode":"allMatches","noMatch":"fail"}
        });
        if with_failed_route {
            transitions["fail"] = json!({"id":"fail","sourceNodeId":"wait","targetNodeId":"err","trigger":"failed","priority":0,"routeGroupId":"fail"});
            groups["fail"] = json!({"id":"fail","mode":"allMatches","noMatch":"fail"});
        }
        definition(
            json!({
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "wait":{"id":"wait","label":"Deadline","scopeId":"root","kind":"wait","waitFor":"timer",
                        "timer":{"deadline":{"kind":"path","root":"input","path":["deadline"]}}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"},
                "err":{"id":"err","label":"Error output","scopeId":"root","kind":"output"}
            }),
            groups,
            transitions,
        )
    }

    #[test]
    fn invalid_timer_deadline_fails_loudly_instead_of_parking() {
        let now = DateTime::<Utc>::UNIX_EPOCH;

        let routed = path_deadline_definition(true);
        routed
            .parsed()
            .validate_publishable()
            .expect("path deadlines stay publishable");
        let mut continuation = ContinuationEnvelope::start(
            routed.parsed(),
            "run".into(),
            "rho:id:team".into(),
            routed.definition_hash(),
            json!({"deadline": "not-a-timestamp"}),
        )
        .expect("continuation should start");
        let outcome = continuation
            .run_until_blocked(routed.parsed(), now)
            .expect("the failed disposition must route, not park");
        let KernelOutcome::Completed(value) = outcome else {
            panic!("expected completion through the failed route, got {outcome:?}")
        };
        assert!(
            value["error"]
                .as_str()
                .expect("the error record carries a reason")
                .contains("is not an RFC 3339 timestamp"),
            "unexpected reason: {value}"
        );
        assert_eq!(value["deadline"], json!("not-a-timestamp"));
        assert!(
            continuation.pending_waits.is_empty(),
            "no unresumable wait may exist"
        );
        assert!(
            continuation
                .events
                .iter()
                .all(|event| event.event_type != "runSuspended"),
            "an uncomputable deadline never suspends"
        );

        let unrouted = path_deadline_definition(false);
        let mut continuation = ContinuationEnvelope::start(
            unrouted.parsed(),
            "run".into(),
            "rho:id:team".into(),
            unrouted.definition_hash(),
            json!({"deadline": 42}),
        )
        .expect("continuation should start");
        let error = continuation
            .run_until_blocked(unrouted.parsed(), now)
            .expect_err("with no failed route the run must fail loudly");
        assert!(
            error
                .to_string()
                .contains("produced disposition failed with no matching route group"),
            "unexpected error: {error:#}"
        );
        assert!(continuation.pending_waits.is_empty());

        let control = path_deadline_definition(true);
        let mut continuation = ContinuationEnvelope::start(
            control.parsed(),
            "run".into(),
            "rho:id:team".into(),
            control.definition_hash(),
            json!({"deadline": "1970-01-01T00:00:20Z"}),
        )
        .expect("continuation should start");
        let KernelOutcome::Suspended(waits) = continuation
            .run_until_blocked(control.parsed(), now)
            .expect("a computable deadline parks normally")
        else {
            panic!("expected the timer suspension")
        };
        assert_eq!(
            waits.first().and_then(|wait| wait.due_at),
            Some(now + chrono::Duration::seconds(20))
        );
    }
}
