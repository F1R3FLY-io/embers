use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use derive_more::Display;
use firefly_client::models::Uri;
use firefly_client::rendering::{Inline, IntoValue, Render, Value};

use crate::domain::agents_teams::compilation::graphl_parsing::Vertex;
use crate::domain::agents_teams::compilation::{AgentRef, Code, Node};
use crate::domain::common::{RegistryDeploy, record_trace};

/// Rholang source for each agent version a team references, resolved before
/// rendering so the renderer itself stays pure.
pub type AgentCodes = BTreeMap<AgentRef, String>;

#[derive(Debug, Clone, Display)]
enum From<'a> {
    #[display("input")]
    Input,
    Channel(&'a str),
}

#[derive(Debug, Clone, Display)]
enum Output<'a> {
    Channel(&'a str),
}

#[derive(Debug, Clone, Render)]
enum NodesTemplate<'a> {
    #[template(path = "agents_teams/nodes/compress.rho")]
    Compress {
        #[template(direct)]
        from: Vec<From<'a>>,

        #[template(direct)]
        output: Output<'a>,

        body: BTreeMap<String, Inline>,
    },

    #[template(path = "agents_teams/nodes/text_model.rho")]
    TextModel {
        #[template(direct)]
        from: From<'a>,

        #[template(direct)]
        output: Output<'a>,
    },

    #[template(path = "agents_teams/nodes/tti_model.rho")]
    TTIModel {
        #[template(direct)]
        from: From<'a>,

        #[template(direct)]
        output: Output<'a>,
    },

    #[template(path = "agents_teams/nodes/tts_model.rho")]
    TTSModel {
        #[template(direct)]
        from: From<'a>,

        #[template(direct)]
        output: Output<'a>,
    },

    #[template(path = "agents_teams/nodes/agent.rho")]
    Agent {
        #[template(direct)]
        from: Vec<From<'a>>,

        #[template(direct)]
        output: Output<'a>,

        /// The agent's Rholang source. Must be `Inline` so it is emitted raw;
        /// a `String` would render as a quoted, escaped Rholang string literal.
        code: Inline,

        /// What is passed to the agent's `main` contract: the single upstream
        /// value, or a map of them when the node has several incoming edges.
        payload: Inline,
    },

    #[template(path = "agents_teams/nodes/output.rho")]
    Output {
        #[template(direct)]
        from: From<'a>,
    },
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/deploy.rho", blocks = ["name"])]
struct DeployAgentTeamTemplate<'a> {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,

    #[template(direct)]
    system_channels: Vec<&'static str>,

    #[template(direct)]
    output_channels: Vec<&'a str>,

    #[template(direct)]
    nodes: Vec<String>,

    #[template(direct)]
    output: bool,
}

fn filter_channels<'a>(from: &[From<'a>]) -> Vec<&'a str> {
    from.iter()
        .filter_map(|f| match f {
            From::Channel(c) => Some(*c),
            From::Input => None,
        })
        .collect()
}

fn get_all_system_channels<'a>(nodes: &BTreeMap<Vertex<'a>, Node<'a>>) -> Vec<&'static str> {
    nodes
        .values()
        .fold(BTreeSet::default(), |mut acc, node| {
            match node {
                Node::Compress { .. } => {}
                Node::TextModel { .. } => {
                    acc.insert("gpt4(`rho:ai:gpt4`)");
                }
                Node::TTIModel { .. } => {
                    acc.insert("dalle3(`rho:ai:dalle3`)");
                }
                Node::TTSModel { .. } => {
                    acc.insert("textToAudio(`rho:ai:textToAudio`)");
                }
                // Agents declare whatever channels they need inside their own
                // code, so nothing has to be bound for them at team scope.
                Node::Agent { .. } => {}
            }
            acc
        })
        .into_iter()
        .collect()
}

/// Human-readable key for the value a node contributes to a downstream join.
///
/// Agent authors index these maps by hand (`input.get("textModel")`), so the
/// label has to be the producing node's name rather than its vertex id. The
/// input node owns no `Node` entry, hence the `None` arm.
fn node_label<'a>(nodes: &BTreeMap<Vertex<'a>, Node<'a>>, vertex: &Vertex<'a>) -> String {
    match nodes.get(vertex) {
        Some(Node::Agent { agent_name, .. }) => lower_camel_case(agent_name),
        Some(Node::TextModel { .. }) => "textModel".to_owned(),
        Some(Node::TTIModel { .. }) => "ttiModel".to_owned(),
        Some(Node::TTSModel { .. }) => "ttsModel".to_owned(),
        Some(Node::Compress { .. }) => "compress".to_owned(),
        None => From::Input.to_string(),
    }
}

/// `"Summarize"` -> `"summarize"`, `"My Agent!"` -> `"myAgent"`. Falls back to
/// `"agent"` so a name made entirely of punctuation still yields a usable key.
fn lower_camel_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut capitalize = false;

    for character in name.chars() {
        if character.is_alphanumeric() {
            if capitalize && !result.is_empty() {
                result.extend(character.to_uppercase());
            } else if result.is_empty() {
                result.extend(character.to_lowercase());
            } else {
                result.push(character);
            }
            capitalize = false;
        } else {
            capitalize = true;
        }
    }

    if result.is_empty() {
        "agent".to_owned()
    } else {
        result
    }
}

/// Labels for a node's incoming edges, in order, made unique by suffixing
/// repeats (`summarize`, `summarize2`) so no contribution is silently dropped
/// when two producers share a name.
fn unique_labels<'a>(nodes: &BTreeMap<Vertex<'a>, Node<'a>>, from: &[Vertex<'a>]) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut labels = Vec::with_capacity(from.len());

    for vertex in from {
        let label = node_label(nodes, vertex);
        let count = counts.entry(label.clone()).or_default();
        *count += 1;
        labels.push(if *count == 1 {
            label
        } else {
            format!("{label}{count}")
        });
    }

    labels
}

/// The Rholang expression holding a given incoming edge's value: the team's
/// `input` when the edge comes straight from the input node, otherwise the
/// variable bound by the join.
fn value_expression(from: &From<'_>) -> String {
    match from {
        From::Input => From::Input.to_string(),
        channel @ From::Channel(_) => format!("{channel}Value"),
    }
}

/// Builds the map a node hands downstream, keyed by producing-node label.
fn labelled_values<'a>(
    nodes: &BTreeMap<Vertex<'a>, Node<'a>>,
    vertex_outputs: &BTreeMap<&Vertex<'a>, String>,
    from: &[Vertex<'a>],
) -> BTreeMap<String, Inline> {
    unique_labels(nodes, from)
        .into_iter()
        .zip(from.iter())
        .map(|(label, vertex)| {
            let expression = value_expression(&get_input_for_vertex(vertex_outputs, vertex));
            (label, Inline::from(expression))
        })
        .collect()
}

fn node_output_channel(index: usize) -> String {
    format!("channel{index}Output")
}

fn get_input_for_vertex<'a, 'b>(
    outputs: &'b BTreeMap<&Vertex<'a>, String>,
    vertex: &Vertex<'a>,
) -> From<'b> {
    outputs
        .get(vertex)
        .map_or(From::Input, |s| From::Channel(s))
}

fn get_output_for_vertex<'a, 'b>(
    outputs: &'b BTreeMap<&Vertex<'a>, String>,
    vertex: &'b Vertex<'a>,
) -> Output<'b> {
    outputs
        .get(&vertex)
        .map_or(Output::Channel("devNull"), |s| Output::Channel(s))
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(code),
    err(Debug),
    ret(Display, level = "trace")
)]
pub fn render(
    code: Code<'_>,
    deploy: RegistryDeploy,
    agent_codes: &AgentCodes,
) -> anyhow::Result<String> {
    record_trace!(code);

    let vertex_outputs: BTreeMap<_, _> = code
        .nodes
        .iter()
        .filter_map(|(vertex, node)| node.output().then_some(vertex))
        .enumerate()
        .map(|(index, v)| (v, node_output_channel(index)))
        .collect();

    let output = code
        .output
        .as_ref()
        .map(|v| get_input_for_vertex(&vertex_outputs, &v.from))
        .map(|from| NodesTemplate::Output { from });

    let nodes = code
        .nodes
        .iter()
        .map(|(vertex, node)| match node {
            Node::Compress { from, .. } => Ok(NodesTemplate::Compress {
                from: from
                    .iter()
                    .map(|from| get_input_for_vertex(&vertex_outputs, from))
                    .collect(),
                output: get_output_for_vertex(&vertex_outputs, vertex),
                body: labelled_values(&code.nodes, &vertex_outputs, from),
            }),
            Node::Agent {
                agent_id,
                agent_version,
                from,
                ..
            } => {
                let key = AgentRef {
                    id: agent_id.clone(),
                    version: agent_version.clone(),
                };
                let source = agent_codes.get(&key).with_context(|| {
                    format!("no code resolved for agent {agent_id} version {agent_version}")
                })?;

                // A single upstream edge is handed over as the value itself;
                // several are joined into a map keyed by producing node.
                let payload = match from.as_slice() {
                    [single] => value_expression(&get_input_for_vertex(&vertex_outputs, single)),
                    _ => Value::Map(
                        labelled_values(&code.nodes, &vertex_outputs, from)
                            .into_iter()
                            .map(|(label, value)| (label, value.into_value()))
                            .collect(),
                    )
                    .to_string(),
                };

                Ok(NodesTemplate::Agent {
                    from: from
                        .iter()
                        .map(|from| get_input_for_vertex(&vertex_outputs, from))
                        .collect(),
                    output: get_output_for_vertex(&vertex_outputs, vertex),
                    code: Inline::from(source.clone()),
                    payload: Inline::from(payload),
                })
            }
            Node::TextModel { from, .. } => Ok(NodesTemplate::TextModel {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }),
            Node::TTIModel { from, .. } => Ok(NodesTemplate::TTIModel {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }),
            Node::TTSModel { from, .. } => Ok(NodesTemplate::TTSModel {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }),
        })
        .chain(output.map(Ok))
        .map(|node| node.and_then(|node| node.render().map_err(Into::into)))
        .collect::<anyhow::Result<_>>()?;

    let output_channels = vertex_outputs.values().map(AsRef::as_ref).collect();

    DeployAgentTeamTemplate {
        env_uri: deploy.uri_pub_key.into(),
        version: deploy.version,
        public_key: deploy.uri_pub_key.serialize_uncompressed().into(),
        sig: deploy.signature,
        system_channels: get_all_system_channels(&code.nodes),
        output_channels,
        nodes,
        output: code.output.is_some(),
    }
    .render()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::domain::agents_teams::compilation::graphl_parsing::Output as GraphOutput;
    use crate::domain::test_helpers::test_public_key;

    const SUMMARIZE_SOURCE: &str = "contract main(@input, return) = {\n  new gpt4(`rho:ai:gpt4`) in {\n    gpt4!(\"Summarize: \" ++ input, *return)\n  }\n}";

    fn deploy() -> RegistryDeploy {
        RegistryDeploy {
            timestamp: Utc::now(),
            version: 1,
            uri_pub_key: test_public_key(),
            signature: vec![1, 2, 3, 4],
        }
    }

    fn agent_node<'a>(from: Vec<Vertex<'a>>, output: bool) -> Node<'a> {
        Node::Agent {
            agent_id: "agent-id".to_owned(),
            agent_version: "agent-version".to_owned(),
            agent_name: "Summarize".to_owned(),
            from,
            output,
        }
    }

    fn agent_codes() -> AgentCodes {
        AgentCodes::from([(
            AgentRef {
                id: "agent-id".to_owned(),
                version: "agent-version".to_owned(),
            },
            SUMMARIZE_SOURCE.to_owned(),
        )])
    }

    #[test]
    fn splices_agent_source_verbatim_and_unquoted() {
        let agent: Vertex<'_> = "aAgent".into();
        let code = Code {
            nodes: BTreeMap::from([(agent, agent_node(vec!["aInput".into()], true))]),
            output: Some(GraphOutput { from: agent }),
        };

        let rendered =
            render(code, deploy(), &agent_codes()).expect("rendering the team should succeed");

        // The team template re-indents the block, so compare on the distinctive
        // lines rather than the whole literal.
        assert!(
            rendered.contains("contract main(@input, return) = {")
                && rendered.contains(r#"gpt4!("Summarize: " ++ input, *return)"#),
            "agent source should be spliced into the team, got:\n{rendered}"
        );
        assert!(
            !rendered.contains(r#"\"Summarize: \""#),
            "agent source must be inlined raw, never escaped into a Rholang string:\n{rendered}"
        );
        assert!(
            rendered.contains("new main, agentReturn in"),
            "agent should be wrapped in its own scope:\n{rendered}"
        );
    }

    #[test]
    fn single_input_passes_the_value_itself() {
        let agent: Vertex<'_> = "aAgent".into();
        let code = Code {
            nodes: BTreeMap::from([(agent, agent_node(vec!["aInput".into()], true))]),
            output: Some(GraphOutput { from: agent }),
        };

        let rendered =
            render(code, deploy(), &agent_codes()).expect("rendering the team should succeed");

        assert!(
            rendered.contains("main!(input, *agentReturn)"),
            "a lone incoming edge should be handed over as the value:\n{rendered}"
        );
    }

    #[test]
    fn multiple_inputs_are_joined_into_a_map_keyed_by_producer() {
        let text: Vertex<'_> = "aText".into();
        let tti: Vertex<'_> = "aTti".into();
        let agent: Vertex<'_> = "aAgent".into();

        let code = Code {
            nodes: BTreeMap::from([
                (
                    text,
                    Node::TextModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (
                    tti,
                    Node::TTIModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (agent, agent_node(vec![text, tti], true)),
            ]),
            output: Some(GraphOutput { from: agent }),
        };

        let rendered =
            render(code, deploy(), &agent_codes()).expect("rendering the team should succeed");

        assert!(
            rendered.contains(r#""textModel""#) && rendered.contains(r#""ttiModel""#),
            "the joined map should be keyed by producing node, not vertex id:\n{rendered}"
        );
        assert!(
            !rendered.contains(r#""aText""#),
            "vertex ids must not leak into the map keys:\n{rendered}"
        );
        assert!(
            rendered.contains('&'),
            "several incoming edges should be joined:\n{rendered}"
        );
        assert!(
            rendered.contains("main!({"),
            "the joined map should be handed to the agent's entry contract:\n{rendered}"
        );
    }

    #[test]
    fn compress_keys_are_producer_labels_not_vertex_ids() {
        let text: Vertex<'_> = "aText".into();
        let tts: Vertex<'_> = "aTts".into();
        let compress: Vertex<'_> = "aCompress".into();

        let code = Code {
            nodes: BTreeMap::from([
                (
                    text,
                    Node::TextModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (
                    tts,
                    Node::TTSModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (
                    compress,
                    Node::Compress {
                        from: vec![text, tts],
                        output: true,
                    },
                ),
            ]),
            output: Some(GraphOutput { from: compress }),
        };

        let rendered = render(code, deploy(), &AgentCodes::default())
            .expect("rendering the team should succeed");

        assert!(
            rendered.contains(r#""textModel""#) && rendered.contains(r#""ttsModel""#),
            "compress should key its map by producing node:\n{rendered}"
        );
        assert!(
            !rendered.contains(r#""aText""#) && !rendered.contains(r#""aTts""#),
            "compress must no longer key its map by vertex id:\n{rendered}"
        );
    }

    #[test]
    fn duplicate_producer_labels_are_disambiguated() {
        let first: Vertex<'_> = "aOne".into();
        let second: Vertex<'_> = "aTwo".into();
        let agent: Vertex<'_> = "aAgent".into();

        let code = Code {
            nodes: BTreeMap::from([
                (
                    first,
                    Node::TextModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (
                    second,
                    Node::TextModel {
                        from: "aInput".into(),
                        output: true,
                    },
                ),
                (agent, agent_node(vec![first, second], true)),
            ]),
            output: Some(GraphOutput { from: agent }),
        };

        let rendered =
            render(code, deploy(), &agent_codes()).expect("rendering the team should succeed");

        assert!(
            rendered.contains(r#""textModel""#) && rendered.contains(r#""textModel2""#),
            "two producers sharing a label must not collapse onto one key:\n{rendered}"
        );
    }

    #[test]
    fn missing_agent_code_is_reported() {
        let agent: Vertex<'_> = "aAgent".into();
        let code = Code {
            nodes: BTreeMap::from([(agent, agent_node(vec!["aInput".into()], true))]),
            output: Some(GraphOutput { from: agent }),
        };

        let error = render(code, deploy(), &AgentCodes::default())
            .expect_err("rendering without the agent's code should fail");

        let message = format!("{error:#}");
        assert!(
            message.contains("agent-id") && message.contains("agent-version"),
            "the error should name the unresolved agent, got: {message}"
        );
    }

    #[test]
    fn agents_bind_their_own_system_channels() {
        let agent: Vertex<'_> = "aAgent".into();
        let code = Code {
            nodes: BTreeMap::from([(agent, agent_node(vec!["aInput".into()], true))]),
            output: Some(GraphOutput { from: agent }),
        };

        let rendered =
            render(code, deploy(), &agent_codes()).expect("rendering the team should succeed");

        assert!(
            !rendered.contains("gpt4(`rho:ai:gpt4`),"),
            "a team of only agents should not bind system channels at team scope:\n{rendered}"
        );
    }

    #[test]
    fn labels_are_lower_camel_cased() {
        assert_eq!(lower_camel_case("Summarize"), "summarize");
        assert_eq!(lower_camel_case("My Agent"), "myAgent");
        assert_eq!(lower_camel_case("caption-writer"), "captionWriter");
        assert_eq!(lower_camel_case("!!!"), "agent");
    }
}
