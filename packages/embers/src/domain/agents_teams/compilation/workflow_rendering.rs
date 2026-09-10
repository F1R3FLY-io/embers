use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, bail};
use firefly_client::models::Uri;
use firefly_client::rendering::Render;
use sha2::Digest;

use crate::domain::agents_teams::compilation::{AgentCodes, AgentRef};
use crate::domain::agents_teams::workflow::{
    ActivityReference,
    AgentProtocol,
    BuiltinActivity,
    Comparison,
    Condition,
    Definition,
    JoinPolicy,
    LateArrival,
    LoopPolicy,
    NoMatch,
    NodeKind,
    RouteGroup,
    RoutingMode,
    ValueExpression,
    ValuePath,
    ValueRoot,
    WaitKind,
    WorkflowDefinition,
    WorkflowNode,
    WorkflowTransition,
};
use crate::domain::common::RegistryDeploy;

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/deploy_workflow.rho", blocks = ["name"])]
struct DeployWorkflowTemplate {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
    definition_json: String,
    definition_hash: String,
    #[template(direct)]
    deployment_bindings: String,

    #[template(direct)]
    system_channels: Vec<&'static str>,
    #[template(direct)]
    private_channels: Vec<String>,
    #[template(direct)]
    initializers: Vec<String>,
    #[template(direct)]
    contracts: Vec<String>,
    #[template(direct)]
    starts: Vec<String>,
    #[template(direct)]
    guard_dispatch: String,
    #[template(direct)]
    activity_dispatch: String,
}

/// Compile a validated Workflow Definition v2 into one pure-Rholang process.
///
/// Every node and router is a persistent contract, so a token may revisit it.
/// Loop counters travel with the token and are incremented on the bounded
/// transition itself.  This is intentionally unlike the legacy `GraphL` lowering,
/// whose one-shot consumers are correct only for DAGs.
pub fn render(
    workflow: &WorkflowDefinition,
    deploy: RegistryDeploy,
    agent_codes: &AgentCodes,
) -> anyhow::Result<String> {
    render_with_containers(workflow, deploy, agent_codes, &BTreeMap::new())
}

pub fn render_with_containers(
    workflow: &WorkflowDefinition,
    deploy: RegistryDeploy,
    agent_codes: &AgentCodes,
    bindings: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let definition = workflow.parsed();
    let names = definition
        .nodes
        .keys()
        .enumerate()
        .map(|(index, id)| (id.clone(), NodeNames::new(index)))
        .collect::<BTreeMap<_, _>>();

    let incoming = definition.transitions.values().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut incoming, transition| {
            incoming
                .entry(transition.target_node_id.clone())
                .or_default()
                .insert(transition.source_node_id.clone());
            incoming
        },
    );

    let mut private_channels = names
        .values()
        .flat_map(|node| [node.node.clone(), node.route.clone()])
        .collect::<Vec<_>>();
    let mut initializers = Vec::new();
    let mut contracts = Vec::new();

    for node in definition.nodes.values() {
        let node_names = &names[&node.id];
        if node.join.is_some() {
            private_channels.push(node_names.join.clone());
            initializers.push(match &node.join {
                Some(JoinPolicy::Race { .. }) => format!("{}!(false)", node_names.join),
                _ => format!("{}!({{}})", node_names.join),
            });
        }

        contracts.push(render_node_contract(
            definition,
            node,
            node_names,
            incoming.get(&node.id).map_or(0, BTreeSet::len),
            &names,
            agent_codes,
            bindings,
        )?);
        contracts.push(render_router_contract(
            definition, node, node_names, &names,
        )?);
    }

    let entry_scope = &definition.scopes[&definition.entry_scope_id];
    let starts = entry_scope
        .initial_node_ids
        .iter()
        .map(|id| {
            names
                .get(id)
                .map(|node| {
                    format!(
                        r#"{}!(input, {{"input": input, "state": {{}}, "iterations": {{}}}}, "entry")"#,
                        node.node
                    )
                })
                .with_context(|| format!("entry node {id} has no runtime channel"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if starts.is_empty() {
        bail!(
            "entry scope {} has no initial nodes",
            definition.entry_scope_id
        );
    }

    DeployWorkflowTemplate {
        env_uri: deploy.uri_pub_key.into(),
        version: deploy.version,
        public_key: deploy.uri_pub_key.serialize_uncompressed().into(),
        sig: deploy.signature,
        definition_json: workflow.json_ref().to_owned(),
        definition_hash: workflow.definition_hash(),
        deployment_bindings: serde_json::to_string(bindings)?,
        system_channels: system_channels(definition),
        private_channels,
        initializers,
        contracts,
        starts,
        guard_dispatch: render_guard_dispatch(definition)?,
        activity_dispatch: render_activity_dispatch(definition, agent_codes, bindings)?,
    }
    .render()
    .map_err(Into::into)
}

#[derive(Debug)]
struct NodeNames {
    node: String,
    route: String,
    join: String,
}

impl NodeNames {
    fn new(index: usize) -> Self {
        Self {
            node: format!("workflowNode{index}"),
            route: format!("workflowRoute{index}"),
            join: format!("workflowJoin{index}"),
        }
    }
}

pub(crate) fn system_channels(definition: &Definition) -> Vec<&'static str> {
    let mut result = BTreeSet::new();
    for node in definition.nodes.values() {
        if let NodeKind::Activity { activity } = &node.kind {
            match activity {
                ActivityReference::Builtin {
                    name: BuiltinActivity::TextModel,
                } => {
                    result.insert("gpt4(`rho:ai:gpt4`)");
                }
                ActivityReference::Builtin {
                    name: BuiltinActivity::TtiModel,
                } => {
                    result.insert("dalle3(`rho:ai:dalle3`)");
                }
                ActivityReference::Builtin {
                    name: BuiltinActivity::TtsModel,
                } => {
                    result.insert("textToAudio(`rho:ai:textToAudio`)");
                }
                ActivityReference::Rholang { .. } => {
                    result.insert("rl(`rho:registry:lookup`)");
                }
                _ => {}
            }
        }
    }
    result.into_iter().collect()
}

fn render_node_contract(
    definition: &Definition,
    node: &WorkflowNode,
    names: &NodeNames,
    incoming_count: usize,
    all_names: &BTreeMap<String, NodeNames>,
    agent_codes: &AgentCodes,
    bindings: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let body = if let Some(uri) = bindings.get(&node.id) {
        let completion = match &node.kind {
            NodeKind::Activity {
                activity:
                    ActivityReference::Agent {
                        protocol: AgentProtocol::StructuredV1,
                        ..
                    },
            } => format!(
                "{}!(result.getOrElse(\"output\", Nil), result.getOrElse(\"disposition\", \"completed\"), context)",
                names.route
            ),
            _ => format!("{}!(result, \"completed\", context)", names.route),
        };
        container_call(
            uri,
            &node.id,
            "$value",
            &completion,
            &format!(
                r#"{}!("Deployment container is unavailable", "failed", context)"#,
                names.route
            ),
        )?
    } else {
        render_node_body(definition, node, names, all_names, agent_codes)?
    };
    let contract = match &node.join {
        None | Some(JoinPolicy::Each { .. }) => format!(
            "contract {}(@value, @context, @source) = {{\n{}\n}}",
            names.node,
            indent(&body, 4)
        ),
        Some(JoinPolicy::All { .. }) => {
            if incoming_count == 0 {
                bail!("all-join node {} has no incoming transitions", node.id);
            }
            let updated = "state.set(source, value)".to_string();
            format!(
                "contract {}(@value, @context, @source) = {{\n    for(@state <- {}) {{\n        if ({}.size() >= {}) {{\n            {}!({{}}) |\n{}\n        }} else {{\n            {}!({})\n        }}\n    }}\n}}",
                names.node,
                names.join,
                updated,
                incoming_count,
                names.join,
                indent(&body.replace("$value", &updated), 12),
                names.join,
                updated,
            )
        }
        Some(JoinPolicy::Quorum { quorum, .. }) => {
            if *quorum > incoming_count {
                bail!(
                    "quorum {} on node {} exceeds its {} incoming routes",
                    quorum,
                    node.id,
                    incoming_count
                );
            }
            let updated = "state.set(source, value)";
            format!(
                "contract {}(@value, @context, @source) = {{\n    for(@state <- {}) {{\n        if ({}.size() >= {}) {{\n            {}!({{}}) |\n{}\n        }} else {{\n            {}!({})\n        }}\n    }}\n}}",
                names.node,
                names.join,
                updated,
                quorum,
                names.join,
                indent(&body.replace("$value", updated), 12),
                names.join,
                updated,
            )
        }
        Some(JoinPolicy::Race { late, .. }) => {
            let late_action = match late {
                LateArrival::Discard => "devNull!(Nil)".to_owned(),
                LateArrival::Cancel => format!(
                    r#"return!({{"disposition":"cancelled","nodeId":{}}})"#,
                    rho_string(&node.id)
                ),
                LateArrival::Error => format!(
                    r#"return!({{"disposition":"failed","nodeId":{},"error":"late race arrival"}})"#,
                    rho_string(&node.id)
                ),
            };
            format!(
                "contract {}(@value, @context, @source) = {{\n    for(@won <- {}) {{\n        if (won) {{\n            {}!(true) |\n            {}\n        }} else {{\n            {}!(true) |\n{}\n        }}\n    }}\n}}",
                names.node,
                names.join,
                names.join,
                late_action,
                names.join,
                indent(&body, 12),
            )
        }
        Some(JoinPolicy::Reduce { .. }) => format!(
            "contract {}(@value, @context, @source) = {{ return!({{\"disposition\":\"suspended\",\"reason\":\"durableReduce\",\"nodeId\":{},\"value\":value}}) }}",
            names.node,
            rho_string(&node.id),
        ),
    };
    Ok(contract.replace("$value", "value"))
}

fn render_node_body(
    definition: &Definition,
    node: &WorkflowNode,
    names: &NodeNames,
    all_names: &BTreeMap<String, NodeNames>,
    agent_codes: &AgentCodes,
) -> anyhow::Result<String> {
    let route = |value: &str, disposition: &str| {
        format!(
            "{}!({}, {}, context)",
            names.route,
            value,
            rho_string(disposition),
        )
    };

    Ok(match &node.kind {
        NodeKind::Input | NodeKind::Gateway | NodeKind::Transform { .. } => {
            route("$value", "completed")
        }
        NodeKind::Output | NodeKind::Final => "return!($value)".to_owned(),
        NodeKind::Wait { wait_for, .. } => format!(
            r#"return!({{"disposition":"suspended","nodeId":{},"waitFor":{},"value":$value}})"#,
            rho_string(&node.id),
            rho_string(match wait_for {
                WaitKind::Event => "event",
                WaitKind::Signal => "signal",
                WaitKind::Timer => "timer",
            }),
        ),
        NodeKind::Approval { policy_id, .. } => format!(
            r#"return!({{"disposition":"approvalRequired","nodeId":{},"policyId":{},"value":$value}})"#,
            rho_string(&node.id),
            rho_string(policy_id),
        ),
        NodeKind::History { .. } => route("$value", "completed"),
        NodeKind::Scope { child_scope_id } => {
            let scope = definition.scopes.get(child_scope_id).with_context(|| {
                format!(
                    "scope node {} refers to missing child scope {}",
                    node.id, child_scope_id
                )
            })?;
            if scope.initial_node_ids.is_empty() {
                bail!("child scope {child_scope_id} has no initial nodes");
            }
            scope
                .initial_node_ids
                .iter()
                .map(|id| {
                    all_names
                        .get(id)
                        .map(|target| {
                            format!(
                                "{}!($value, context, {})",
                                target.node,
                                rho_string(node.output_key())
                            )
                        })
                        .with_context(|| {
                            format!("scope {child_scope_id} initial node {id} is missing")
                        })
                })
                .collect::<anyhow::Result<Vec<_>>>()?
                .join(" |\n")
        }
        NodeKind::Activity { activity } => match activity {
            ActivityReference::Agent {
                agent_id,
                agent_version,
                protocol,
            } => {
                let source = agent_codes
                    .get(&AgentRef {
                        id: agent_id.clone(),
                        version: agent_version.clone(),
                    })
                    .with_context(|| {
                        format!("no code resolved for agent {agent_id} version {agent_version}")
                    })?;
                let payload = match protocol {
                    AgentProtocol::SimpleV1 => "$value".to_owned(),
                    AgentProtocol::StructuredV1 => {
                        r#"{"kind":"invoke","input":$value,"context":{"nodeId":"#.to_owned()
                            + &rho_string(&node.id)
                            + "}}"
                    }
                };
                let completion = match protocol {
                    AgentProtocol::SimpleV1 => route("agentResult", "completed"),
                    AgentProtocol::StructuredV1 => format!(
                        "{}!(agentResult.getOrElse(\"output\", Nil), agentResult.getOrElse(\"disposition\", \"completed\"), context)",
                        names.route
                    ),
                };
                format!(
                    "new main, agentReturn in {{\n{}\n    | main!({}, *agentReturn)\n    | for(@agentResult <- agentReturn) {{ {} }}\n}}",
                    indent(source, 4),
                    payload,
                    completion
                )
            }
            ActivityReference::Builtin {
                name: BuiltinActivity::Compress,
            } => route("$value", "completed"),
            ActivityReference::Builtin {
                name: BuiltinActivity::TextModel,
            } => format!(
                "new activityReturn in {{ gpt4!($value, *activityReturn) | for(@result <- activityReturn) {{ {} }} }}",
                route("result", "completed")
            ),
            ActivityReference::Builtin {
                name: BuiltinActivity::TtiModel,
            } => format!(
                "new activityReturn in {{ dalle3!($value, *activityReturn) | for(@result <- activityReturn) {{ {} }} }}",
                route("result", "completed")
            ),
            ActivityReference::Builtin {
                name: BuiltinActivity::TtsModel,
            } => format!(
                "new activityReturn in {{ textToAudio!($value, *activityReturn) | for(@result <- activityReturn) {{ {} }} }}",
                route("result", "completed")
            ),
            ActivityReference::Rholang { uri, protocol } => {
                let payload = match protocol {
                    AgentProtocol::SimpleV1 => "$value".to_owned(),
                    AgentProtocol::StructuredV1 => r#"{"kind":"invoke","input":$value}"#.to_owned(),
                };
                format!(
                    "new lookupReturn, activityReturn in {{ rl!({}, *lookupReturn) | for(@(_, activity) <- lookupReturn) {{ @activity!({}, *activityReturn) | for(@result <- activityReturn) {{ {} }} }} }}",
                    rho_uri(uri)?,
                    payload,
                    route("result", "completed")
                )
            }
            ActivityReference::ForeignTerm { .. } => format!(
                r#"return!({{"disposition":"failed","nodeId":{},"error":"FLT value bridging is unavailable until the governing FIPS is accepted"}})"#,
                rho_string(&node.id)
            ),
            ActivityReference::Workflow { team_id, version } => render_nested_workflow_call(
                team_id,
                version,
                "$value",
                &format!("{}!(result, \"completed\", context)", names.route),
            )?,
        },
    })
}

fn render_guard_dispatch(definition: &Definition) -> anyhow::Result<String> {
    let cases = definition
        .transitions
        .values()
        .filter_map(|transition| match &transition.guard {
            Some(Condition::Rholang { source, inputs, .. }) => Some((transition, source, inputs)),
            _ => None,
        })
        .map(|(transition, source, inputs)| {
            let mut expression = source.clone();
            let mut bindings = inputs.iter().collect::<Vec<_>>();
            bindings.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
            for (name, path) in bindings {
                expression = expression.replace(&format!("${name}"), &render_host_path(path));
            }
            Ok((transition.id.clone(), format!("return!({expression})")))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(render_dispatch(
        "transitionId",
        cases,
        r#"return!({"error":"unknown Rholang guard transition"})"#,
    ))
}

pub(crate) fn render_activity_dispatch(
    definition: &Definition,
    agent_codes: &AgentCodes,
    bindings: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let cases = definition
        .nodes
        .values()
        .filter_map(|node| match &node.kind {
            NodeKind::Activity { activity } => Some((node, activity)),
            _ => None,
        })
        .map(|(node, activity)| {
            let body = if let Some(uri) = bindings.get(&node.id) {
                container_call(uri, &node.id, "input", "return!(result)", r#"return!({"disposition":"failed","error":"Deployment container is unavailable"})"#)
            } else {
                render_activity_host(node, activity, agent_codes)
            }?;
            Ok((node.id.clone(), body))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(render_dispatch(
        "nodeId",
        cases,
        r#"return!({"disposition":"failed","error":"unknown activity node"})"#,
    ))
}

fn render_dispatch(selector: &str, cases: Vec<(String, String)>, fallback: &str) -> String {
    cases
        .into_iter()
        .rev()
        .fold(fallback.to_owned(), |otherwise, (id, body)| {
            format!(
                "if ({selector} == {}) {{\n{}\n}} else {{\n{}\n}}",
                rho_string(&id),
                indent(&body, 4),
                indent(&otherwise, 4),
            )
        })
}

fn render_activity_host(
    node: &WorkflowNode,
    activity: &ActivityReference,
    agent_codes: &AgentCodes,
) -> anyhow::Result<String> {
    Ok(match activity {
        ActivityReference::Agent {
            agent_id,
            agent_version,
            protocol,
        } => {
            let source = agent_codes
                .get(&AgentRef {
                    id: agent_id.clone(),
                    version: agent_version.clone(),
                })
                .with_context(|| {
                    format!("no code resolved for agent {agent_id} version {agent_version}")
                })?;
            let payload = match protocol {
                AgentProtocol::SimpleV1 => "input".to_owned(),
                AgentProtocol::StructuredV1 => format!(
                    r#"{{"kind":"invoke","input":input,"context":{{"nodeId":{}}}}}"#,
                    rho_string(&node.id)
                ),
            };
            format!(
                "new main, activityReturn in {{\n{}\n    | main!({}, *activityReturn)\n    | for(@result <- activityReturn) {{ return!(result) }}\n}}",
                indent(source, 4),
                payload,
            )
        }
        ActivityReference::Builtin {
            name: BuiltinActivity::Compress,
        } => "return!(input)".to_owned(),
        ActivityReference::Builtin {
            name: BuiltinActivity::TextModel,
        } => "new result in { gpt4!(input, *result) | for(@value <- result) { return!(value) } }".to_owned(),
        ActivityReference::Builtin {
            name: BuiltinActivity::TtiModel,
        } => "new result in { dalle3!(input, *result) | for(@value <- result) { return!(value) } }".to_owned(),
        ActivityReference::Builtin {
            name: BuiltinActivity::TtsModel,
        } => "new result in { textToAudio!(input, *result) | for(@value <- result) { return!(value) } }".to_owned(),
        ActivityReference::Rholang { uri, protocol } => {
            let payload = match protocol {
                AgentProtocol::SimpleV1 => "input".to_owned(),
                AgentProtocol::StructuredV1 => r#"{"kind":"invoke","input":input}"#.to_owned(),
            };
            format!(
                "new lookupReturn, activityReturn in {{ rl!({}, *lookupReturn) | for(@(_, activity) <- lookupReturn) {{ @activity!({}, *activityReturn) | for(@result <- activityReturn) {{ return!(result) }} }} }}",
                rho_uri(uri)?,
                payload,
            )
        }
        ActivityReference::Workflow { team_id, version } => {
            render_nested_workflow_call(team_id, version, "input", "return!(result)")?
        }
        ActivityReference::ForeignTerm { language_tag, source, .. } => format!(
            r#"return!({{"disposition":"failed","nodeId":{},"languageTag":{},"sourceHash":{},"error":"FLT value bridging is unavailable until the governing FIPS is accepted"}})"#,
            rho_string(&node.id),
            rho_string(language_tag),
            rho_string(&hex::encode(sha2::Sha256::digest(source.as_bytes()))),
        ),
    })
}

fn render_nested_workflow_call(
    team_uri: &str,
    definition_hash: &str,
    input: &str,
    completion: &str,
) -> anyhow::Result<String> {
    Ok(format!(
        "new nestedTeamCh, manifestReturn, nestedReturn in {{ rl!({}, *nestedTeamCh) | for(@(_, nestedTeam) <- nestedTeamCh) {{ @nestedTeam!(\"control\", \"manifest\", *manifestReturn) | for(@manifest <- manifestReturn) {{ if (manifest.get(\"definitionHash\") == {}) {{ @nestedTeam!({}, *nestedReturn) | for(@result <- nestedReturn) {{ {} }} }} else {{ return!({{\"disposition\":\"failed\",\"error\":\"nested workflow definition hash mismatch\"}}) }} }} }} }}",
        rho_uri(team_uri)?,
        rho_string(definition_hash),
        input,
        completion,
    ))
}

fn render_host_path(path: &ValuePath) -> String {
    let root = match path.root {
        ValueRoot::Input => "input",
        ValueRoot::Output | ValueRoot::Error => "value",
        ValueRoot::State => "state",
    };
    path.path.iter().fold(root.to_owned(), |value, key| {
        format!("{value}.getOrElse({}, Nil)", rho_string(key))
    })
}

fn render_router_contract(
    definition: &Definition,
    node: &WorkflowNode,
    names: &NodeNames,
    all_names: &BTreeMap<String, NodeNames>,
) -> anyhow::Result<String> {
    let mut by_group = BTreeMap::<String, Vec<&WorkflowTransition>>::new();
    for transition in definition
        .transitions
        .values()
        .filter(|transition| transition.source_node_id == node.id)
    {
        by_group
            .entry(transition.route_group_id.clone())
            .or_default()
            .push(transition);
    }
    let groups = by_group
        .into_iter()
        .map(|(group_id, mut transitions)| {
            transitions.sort_by_key(|transition| (transition.priority, transition.id.as_str()));
            let group = &definition.route_groups[&group_id];
            render_route_group(group, &transitions, node, all_names)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let body = if groups.is_empty() {
        "devNull!(Nil)".to_owned()
    } else {
        groups.join(" |\n")
    };
    Ok(format!(
        "contract {}(@value, @disposition, @context) = {{\n{}\n}}",
        names.route,
        indent(&body, 4)
    ))
}

fn render_route_group(
    group: &RouteGroup,
    transitions: &[&WorkflowTransition],
    source: &WorkflowNode,
    names: &BTreeMap<String, NodeNames>,
) -> anyhow::Result<String> {
    let Some(first) = transitions.first() else {
        return Ok("devNull!(Nil)".to_owned());
    };
    if transitions
        .iter()
        .any(|transition| transition.trigger != first.trigger)
    {
        bail!("route group {} mixes trigger dispositions", group.id);
    }
    let no_match = match group.no_match {
        NoMatch::Complete => "return!(value)".to_owned(),
        NoMatch::Skip => "devNull!(Nil)".to_owned(),
        NoMatch::Fail => format!(
            r#"return!({{"disposition":"failed","nodeId":{},"error":{}}})"#,
            rho_string(&source.id),
            rho_string(&format!("route group {} matched no transition", group.id))
        ),
    };
    let guarded = transitions
        .iter()
        .map(|transition| {
            Ok((
                render_guard(transition.guard.as_ref())?,
                render_send(transition, source, names)?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let dispatch = match group.mode {
        RoutingMode::FirstMatch => guarded
            .iter()
            .rev()
            .fold(no_match, |otherwise, (guard, send)| {
                format!("if ({guard}) {{ {send} }} else {{ {otherwise} }}")
            }),
        RoutingMode::AllMatches => {
            let any = guarded
                .iter()
                .map(|(guard, _)| format!("({guard})"))
                .collect::<Vec<_>>()
                .join(" or ");
            let sends = guarded
                .iter()
                .map(|(guard, send)| format!("if ({guard}) {{ {send} }} else {{ devNull!(Nil) }}"))
                .collect::<Vec<_>>()
                .join(" |\n");
            format!(
                "if ({any}) {{\n{}\n}} else {{ {no_match} }}",
                indent(&sends, 4)
            )
        }
    };
    Ok(format!(
        "if (disposition == {}) {{\n{}\n}} else {{ devNull!(Nil) }}",
        rho_string(&first.trigger),
        indent(&dispatch, 4)
    ))
}

fn render_send(
    transition: &WorkflowTransition,
    source: &WorkflowNode,
    names: &BTreeMap<String, NodeNames>,
) -> anyhow::Result<String> {
    let target = names
        .get(&transition.target_node_id)
        .with_context(|| format!("transition {} target has no runtime channel", transition.id))?;
    let source_label = rho_string(source.output_key());
    Ok(match &transition.loop_policy {
        Some(LoopPolicy::Bounded { max_iterations }) => {
            let id = rho_string(&transition.id);
            format!(
                "if (context.getOrElse(\"iterations\", {{}}).getOrElse({id}, 0) < {max_iterations}) {{ {}!(value, context.set(\"iterations\", context.getOrElse(\"iterations\", {{}}).set({id}, context.getOrElse(\"iterations\", {{}}).getOrElse({id}, 0) + 1)), {source_label}) }} else {{ return!({{\"disposition\":\"iterationLimit\",\"transitionId\":{id},\"value\":value}}) }}",
                target.node
            )
        }
        Some(LoopPolicy::Durable { .. }) => {
            let id = rho_string(&transition.id);
            let target_id = rho_string(&transition.target_node_id);
            format!(
                "return!({{\"disposition\":\"suspended\",\"reason\":\"durableYield\",\"transitionId\":{id},\"resumeAtNodeId\":{target_id},\"value\":value}})"
            )
        }
        _ => format!("{}!(value, context, {source_label})", target.node),
    })
}

fn render_guard(condition: Option<&Condition>) -> anyhow::Result<String> {
    Ok(match condition {
        None => "true".to_owned(),
        Some(Condition::Literal { value }) => value.to_string(),
        Some(Condition::All { operands }) => operands
            .iter()
            .map(|operand| render_guard(Some(operand)).map(|guard| format!("({guard})")))
            .collect::<anyhow::Result<Vec<_>>>()?
            .join(" and "),
        Some(Condition::Any { operands }) => operands
            .iter()
            .map(|operand| render_guard(Some(operand)).map(|guard| format!("({guard})")))
            .collect::<anyhow::Result<Vec<_>>>()?
            .join(" or "),
        Some(Condition::Not { operand }) => format!("not ({})", render_guard(Some(operand))?),
        Some(Condition::Exists { expression }) => {
            format!("{} != Nil", render_expression(expression)?)
        }
        Some(Condition::Compare {
            left,
            operator,
            right,
        }) => format!(
            "{} {} {}",
            render_expression(left)?,
            match operator {
                Comparison::Eq => "==",
                Comparison::Ne => "!=",
                Comparison::Gt => ">",
                Comparison::Ge => ">=",
                Comparison::Lt => "<",
                Comparison::Le => "<=",
            },
            render_expression(right)?
        ),
        Some(Condition::Rholang { inputs, source, .. }) => {
            let mut result = source.clone();
            let mut inputs = inputs.iter().collect::<Vec<_>>();
            inputs.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
            for (name, path) in inputs {
                result = result.replace(&format!("${name}"), &render_path(path));
            }
            result
        }
    })
}

fn render_expression(expression: &ValueExpression) -> anyhow::Result<String> {
    match expression {
        ValueExpression::Path { value } => Ok(render_path(value)),
        ValueExpression::Literal { value } => render_literal(value),
    }
}

fn render_path(path: &ValuePath) -> String {
    let root = match path.root {
        ValueRoot::Input => "context.getOrElse(\"input\", Nil)",
        ValueRoot::Output | ValueRoot::Error => "value",
        ValueRoot::State => "context.getOrElse(\"state\", {})",
    };
    path.path.iter().fold(root.to_owned(), |value, key| {
        format!("{value}.getOrElse({}, Nil)", rho_string(key))
    })
}

fn render_literal(value: &serde_json::Value) -> anyhow::Result<String> {
    match value {
        serde_json::Value::Null => Ok("Nil".to_owned()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::String(value) => Ok(rho_string(value)),
        _ => bail!("guard literals must be scalar portable values"),
    }
}

fn rho_uri(uri: &str) -> anyhow::Result<String> {
    if uri.contains('`') || uri.contains('\\') {
        bail!("unsafe Rholang URI literal");
    }
    Ok(format!("`{uri}`"))
}

fn rho_string(value: &str) -> String {
    format!("\"{}\"", escape_rho_string(value))
}
fn escape_rho_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use serde_json::json;

    use super::render;
    use crate::domain::agents_teams::compilation::AgentRef;
    use crate::domain::agents_teams::workflow::WorkflowDefinition;
    use crate::domain::common::RegistryDeploy;
    use crate::domain::test_helpers::test_public_key;

    fn deploy() -> RegistryDeploy {
        RegistryDeploy {
            timestamp: Utc::now(),
            version: 1,
            uri_pub_key: test_public_key(),
            signature: vec![1, 2, 3],
        }
    }

    #[test]
    fn cyclic_transition_uses_persistent_contract_and_counter() {
        let value = json!({
            "schemaVersion":"2.0", "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"string"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{"in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},"loop":{"id":"loop","label":"Loop","scopeId":"root","kind":"gateway"}},
            "policies":{"defaultExecution":"bounded","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{"start":{"id":"start","mode":"allMatches","noMatch":"fail"},"again":{"id":"again","mode":"firstMatch","noMatch":"complete"}},
            "scopes":{"root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"}},
            "transitions":{"start":{"id":"start","sourceNodeId":"in","targetNodeId":"loop","trigger":"completed","priority":0,"routeGroupId":"start"},"again":{"id":"again","sourceNodeId":"loop","targetNodeId":"loop","trigger":"completed","guard":{"kind":"literal","value":true},"priority":0,"routeGroupId":"again","loop":{"maxIterations":3}}},
            "types":{}
        });
        let definition = WorkflowDefinition::new(value).unwrap();
        let rho = render(&definition, deploy(), &BTreeMap::new()).unwrap();
        assert!(rho.contains("contract agentsTeam(@\"control\", @\"manifest\", return)"));
        assert!(!rho.contains("contract agentTeam("));
        assert!(rho.contains("contract workflowNode1"));
        assert!(rho.contains("getOrElse(\"iterations\", {}).getOrElse(\"again\", 0) < 3"));
        assert!(rho.contains("workflowNode1!(value, context.set(\"iterations\""));
    }

    #[test]
    fn structured_invocation_quotes_the_node_id() {
        let value = json!({
            "schemaVersion":"2.0", "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"string"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "draft":{"id":"draft","label":"Draft","scopeId":"root","kind":"activity","activity":{"kind":"agent","agentId":"agent-1","agentVersion":"v1","protocol":"structured-v1"}},
                "out":{"id":"out","label":"Output","scopeId":"root","kind":"output"}
            },
            "policies":{"defaultExecution":"bounded","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{
                "start":{"id":"start","mode":"allMatches","noMatch":"fail"},
                "finish":{"id":"finish","mode":"allMatches","noMatch":"fail"}
            },
            "scopes":{"root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"}},
            "transitions":{
                "start":{"id":"start","sourceNodeId":"in","targetNodeId":"draft","trigger":"completed","priority":0,"routeGroupId":"start"},
                "finish":{"id":"finish","sourceNodeId":"draft","targetNodeId":"out","trigger":"completed","priority":0,"routeGroupId":"finish"}
            },
            "types":{}
        });
        let definition = WorkflowDefinition::new(value).unwrap();
        let codes = BTreeMap::from([(
            AgentRef {
                id: "agent-1".to_owned(),
                version: "v1".to_owned(),
            },
            "contract main(@invocation, return) = { return!({\"output\": invocation, \"disposition\": \"completed\"}) }".to_owned(),
        )]);
        let rho = render(&definition, deploy(), &codes).unwrap();
        // The invocation record must carry the node id as a quoted Rholang
        // string; the bare form parses as a variable and the stray closing
        // quote corrupts the string context of the whole compiled contract.
        assert!(rho.contains(r#""context":{"nodeId":"draft"}}"#));
        assert!(!rho.contains(r#""nodeId":draft"#));
    }

    #[test]
    fn boolean_guards_use_expression_operators_not_pattern_connectives() {
        let value = json!({
            "schemaVersion":"2.0", "entryScopeId":"root",
            "interface":{"inputs":{"input":{"kind":"string"}},"outputs":{"output":{"kind":"legacy"}}},
            "nodes":{
                "in":{"id":"in","label":"Input","scopeId":"root","kind":"input"},
                "left":{"id":"left","label":"Left","scopeId":"root","kind":"output"},
                "right":{"id":"right","label":"Right","scopeId":"root","kind":"output"}
            },
            "policies":{"defaultExecution":"bounded","visibility":"private"},
            "presentation":{"edges":{},"nodes":{}},
            "routeGroups":{"fork":{"id":"fork","mode":"allMatches","noMatch":"fail"}},
            "scopes":{"root":{"id":"root","label":"Root","kind":"root","initialNodeIds":["in"],"entryActivities":[],"exitActivities":[],"history":"none"}},
            "transitions":{
                "left":{"id":"left","sourceNodeId":"in","targetNodeId":"left","trigger":"completed","priority":0,"routeGroupId":"fork","guard":{"kind":"all","operands":[{"kind":"literal","value":true},{"kind":"literal","value":true}]}},
                "right":{"id":"right","sourceNodeId":"in","targetNodeId":"right","trigger":"completed","priority":1,"routeGroupId":"fork","guard":{"kind":"any","operands":[{"kind":"literal","value":false},{"kind":"not","operand":{"kind":"literal","value":false}}]}}
            },
            "types":{}
        });
        let definition = WorkflowDefinition::new(value).unwrap();
        let rho = render(&definition, deploy(), &BTreeMap::new()).unwrap();

        assert!(rho.contains(" and "));
        assert!(rho.contains(" or "));
        assert!(rho.contains("not (false)"));
        assert!(!rho.contains("&&"));
        assert!(!rho.contains("||"));
        assert!(!rho.contains(" /\\ "));
        assert!(!rho.contains(" \\/ "));
    }
}

fn container_call(
    uri: &str,
    node_id: &str,
    input: &str,
    completion: &str,
    failure: &str,
) -> anyhow::Result<String> {
    Ok(format!(
        "new containerLookup(`rho:registry:lookup`), foundContainer, containerResult in {{ containerLookup!({}, *foundContainer) | for(@entry <- foundContainer) {{ match entry {{ (_, container) => {{ @container!(\"activity\", {}, {}, *containerResult) | for(@result <- containerResult) {{ {} }} }} _ => {{ {} }} }} }} }}",
        rho_uri(uri)?,
        rho_string(node_id),
        input,
        completion,
        failure
    ))
}
