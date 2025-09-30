use std::collections::BTreeMap;

use askama::{Result, Template};
use derive_more::Display;
use firefly_client::rendering::{Inline, IntoValue, Value};

use crate::ai_agents_teams::compilation::graphl_parsing::Vertex;
use crate::ai_agents_teams::compilation::{Code, Node};
use crate::common::tracing::record_trace;

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

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/nodes/compress.rho", escape = "none")]
struct CompressTemplate<'a> {
    from: Vec<From<'a>>,
    output: Output<'a>,
    body: Value,
}

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/nodes/text_model.rho", escape = "none")]
struct TextModelTemplate<'a> {
    from: From<'a>,
    output: Output<'a>,
}

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/nodes/tti_model.rho", escape = "none")]
struct TTIModelTemplate<'a> {
    from: From<'a>,
    output: Output<'a>,
}

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/nodes/tts_model.rho", escape = "none")]
struct TTSModelTemplate<'a> {
    from: From<'a>,
    output: Output<'a>,
}

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/nodes/output.rho", escape = "none")]
struct OutputTemplate<'a> {
    from: From<'a>,
}

#[derive(Debug, Clone, askama::Template)]
#[template(path = "ai_agents_teams/deploy_agent_team.rho", escape = "none")]
struct DeployAgentTeamTemplate<'a> {
    name: &'a str,
    system_channels: Vec<&'static str>,
    output_channels: Vec<String>,
    steps: Vec<String>,
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

fn get_all_system_channels(nodes: &[Node<'_>]) -> Vec<&'static str> {
    [
        nodes
            .iter()
            .any(|node| matches!(node, Node::TextModel { .. }))
            .then_some("gpt4(`rho:ai:gpt4`)"),
        nodes
            .iter()
            .any(|node| matches!(node, Node::TTIModel { .. }))
            .then_some("dalle3(`rho:ai:dalle3`)"),
        nodes
            .iter()
            .any(|node| matches!(node, Node::TTSModel { .. }))
            .then_some("textToAudio(`rho:ai:textToAudio`)"),
    ]
    .into_iter()
    .flatten()
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
    ret(Debug, level = "trace")
)]
pub fn render_agent_team(name: &str, code: Code<'_>) -> anyhow::Result<String> {
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
        .map(|from| OutputTemplate { from }.render());

    let steps = code
        .nodes
        .iter()
        .map(|(vertex, node)| match node {
            Node::Compress { from, .. } => Ok(CompressTemplate {
                from: from
                    .iter()
                    .map(|from| get_input_for_vertex(&vertex_outputs, from))
                    .collect(),
                output: get_output_for_vertex(&vertex_outputs, vertex),
                body: Value::Map(
                    from.iter()
                        .map(|from| {
                            (
                                (*from.as_ref()).to_owned(),
                                Inline(get_input_for_vertex(&vertex_outputs, from).to_string())
                                    .into_value(),
                            )
                        })
                        .collect(),
                ),
            }
            .render()?),
            Node::TextModel { from, .. } => Ok(TextModelTemplate {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }
            .render()?),
            Node::TTIModel { from, .. } => Ok(TTIModelTemplate {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }
            .render()?),
            Node::TTSModel { from, .. } => Ok(TTSModelTemplate {
                from: get_input_for_vertex(&vertex_outputs, from),
                output: get_output_for_vertex(&vertex_outputs, vertex),
            }
            .render()?),
        })
        .chain(output)
        .collect::<Result<_, _>>()?;

    let output_channels = vertex_outputs.into_values().collect();

    let nodes: Vec<_> = code.nodes.into_values().collect();

    DeployAgentTeamTemplate {
        name,
        system_channels: get_all_system_channels(&nodes),
        output_channels,
        steps,
        output: code.output.is_some(),
    }
    .render()
    .map_err(Into::into)
}
