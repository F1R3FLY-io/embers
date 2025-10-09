use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, anyhow};
use derive_more::{AsRef, Display, From};
use serde::Deserialize;

use crate::ai_agents_teams::models::Graph;
use crate::common::tracing::record_trace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, AsRef, Display)]
pub struct Vertex<'a>(&'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, AsRef, Display)]
pub struct Name<'a>(&'a str);

#[derive(Debug, Clone, Default)]
struct FlatGraph<'a> {
    vertexes: BTreeSet<Vertex<'a>>,
    bindings: BTreeMap<Name<'a>, Vertex<'a>>,
    bindings_reverse: BTreeMap<Vertex<'a>, BTreeSet<Name<'a>>>,
    edges: BTreeSet<(Name<'a>, Name<'a>)>,
    contexts: BTreeMap<Name<'a>, &'a str>,
}

#[derive(Debug, Clone)]
struct Visitor;

impl<'a> graphl_parser::Visitor<'a, FlatGraph<'a>, anyhow::Error> for Visitor {
    fn visit_vertex(
        &self,
        mut acc: FlatGraph<'a>,
        vertex: &'a graphl_parser::ast::GVertex,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        let name = match &vertex.vertex.name {
            graphl_parser::ast::Name::Wildcard => {
                return Err(anyhow!("Wildcar is not supported in vertex name"));
            }
            graphl_parser::ast::Name::VVar { value } => Vertex(value),
            graphl_parser::ast::Name::GVar { value } => Vertex(value),
            graphl_parser::ast::Name::QuoteGraph { .. } => {
                return Err(anyhow!("QuoteGraph is not supported in vertex name"));
            }
            graphl_parser::ast::Name::QuoteVertex { .. } => {
                return Err(anyhow!("QuoteVertex is not supported in vertex name"));
            }
        };

        acc.vertexes.insert(name);
        Ok(acc)
    }

    fn visit_nominate(
        &self,
        mut acc: FlatGraph<'a>,
        binding: &'a graphl_parser::ast::Binding,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        let (binding, name) = match &binding.vertex.name {
            graphl_parser::ast::Name::Wildcard => {
                return Err(anyhow!("Wildcar is not supported in binding vertex name"));
            }
            graphl_parser::ast::Name::VVar { value } => (Name(&binding.var), Vertex(value)),
            graphl_parser::ast::Name::GVar { value } => (Name(&binding.var), Vertex(value)),
            graphl_parser::ast::Name::QuoteGraph { .. } => {
                return Err(anyhow!(
                    "QuoteGraph is not supported in binding vertex name"
                ));
            }
            graphl_parser::ast::Name::QuoteVertex { .. } => {
                return Err(anyhow!(
                    "QuoteVertex is not supported in binding vertex name"
                ));
            }
        };

        acc.vertexes.insert(name);
        match acc.bindings.entry(binding) {
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(name);
            }
            Entry::Occupied(occupied_entry) => {
                if occupied_entry.get() != &name {
                    return Err(anyhow!(
                        "Same binding name used twice for different nodes: {name} and {}",
                        occupied_entry.get()
                    ));
                }
            }
        }

        acc.bindings_reverse
            .entry(name)
            .or_default()
            .insert(binding);

        Ok(acc)
    }

    fn visit_edge_anon(
        &self,
        mut acc: FlatGraph<'a>,
        edge: &'a graphl_parser::ast::GEdgeAnon,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        acc.edges
            .insert((Name(&edge.binding_1.var), Name(&edge.binding_2.var)));
        Ok(acc)
    }

    fn visit_edge_named(
        &self,
        mut acc: FlatGraph<'a>,
        edge: &'a graphl_parser::ast::GEdgeNamed,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        acc.edges
            .insert((Name(&edge.binding_1.var), Name(&edge.binding_2.var)));
        Ok(acc)
    }

    fn visit_context(
        &self,
        mut acc: FlatGraph<'a>,
        context: &'a graphl_parser::ast::GContext,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        let name = match &context.name {
            graphl_parser::ast::Name::Wildcard => {
                return Err(anyhow!("Wildcar is not supported in context name"));
            }
            graphl_parser::ast::Name::VVar { value } => Name(value),
            graphl_parser::ast::Name::GVar { value } => Name(value),
            graphl_parser::ast::Name::QuoteGraph { .. } => {
                return Err(anyhow!("QuoteGraph is not supported in context name"));
            }
            graphl_parser::ast::Name::QuoteVertex { .. } => {
                return Err(anyhow!("QuoteVertex is not supported in context name"));
            }
        };

        match acc.contexts.entry(name) {
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(&context.string);
            }
            Entry::Occupied(occupied_entry) => {
                if occupied_entry.get() != &context.string {
                    return Err(anyhow!(
                        "Same context name used twice for different entries"
                    ));
                }
            }
        }
        Ok(acc)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum NodeContext {
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "output")]
    Output,
    #[serde(rename = "compress")]
    Compress,
    #[serde(rename = "text-model")]
    TextModel,
    #[serde(rename = "tti-model")]
    TTIModel,
    #[serde(rename = "tts-model")]
    TTSModel,
}

#[derive(Debug, Clone, Default)]
struct PartialCode<'a> {
    input: Option<Input>,
    nodes: BTreeMap<Vertex<'a>, Node<'a>>,
    output: Option<Output<'a>>,
}

#[derive(Debug, Clone)]
pub struct Input;

#[derive(Debug, Clone)]
pub struct Output<'a> {
    pub from: Vertex<'a>,
}

#[derive(Debug, Clone)]
pub enum Node<'a> {
    Compress { from: Vec<Vertex<'a>>, output: bool },
    TextModel { from: Vertex<'a>, output: bool },
    TTIModel { from: Vertex<'a>, output: bool },
    TTSModel { from: Vertex<'a>, output: bool },
}

impl Node<'_> {
    pub const fn output(&self) -> bool {
        match self {
            Node::Compress { output, .. } => *output,
            Node::TextModel { output, .. } => *output,
            Node::TTIModel { output, .. } => *output,
            Node::TTSModel { output, .. } => *output,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Code<'a> {
    pub nodes: BTreeMap<Vertex<'a>, Node<'a>>,
    pub output: Option<Output<'a>>,
}

fn resolve_context<'a>(flat: &FlatGraph<'a>, node: Vertex<'a>) -> anyhow::Result<NodeContext> {
    let mut contexts = flat
        .bindings_reverse
        .get(&node)
        .with_context(|| format!("node {node} missing binding needed for context"))?
        .iter()
        .filter_map(|binding| flat.contexts.get(binding))
        .fuse();

    let context = match (contexts.next(), contexts.next()) {
        (Some(context), None) => context,
        (None, _) => return Err(anyhow!("node {node} is missing context")),
        (Some(_), Some(_)) => return Err(anyhow!("node {node} has multiple contexts")),
    };

    serde_json::from_str(context).with_context(|| "node {node} has invalid context")
}

fn resolve_froms<'a>(
    flat: &FlatGraph<'a>,
    node: Vertex<'a>,
) -> anyhow::Result<impl Iterator<Item = Vertex<'a>>> {
    let bindings = flat
        .bindings_reverse
        .get(&node)
        .with_context(|| format!("node {node} doesnt have bindings"))?;

    Ok(flat
        .edges
        .iter()
        .filter_map(|(from, to)| bindings.iter().any(|b| b == to).then_some(from))
        .filter_map(|from| flat.bindings.get(from).copied()))
}

fn resolve_from<'a>(flat: &FlatGraph<'a>, node: Vertex<'a>) -> anyhow::Result<Vertex<'a>> {
    let mut froms = resolve_froms(flat, node)?.fuse();

    match (froms.next(), froms.next()) {
        (Some(from), None) => Ok(from),
        (None, _) => Err(anyhow!("node {node} has no inputs")),
        (Some(_), Some(_)) => Err(anyhow!(
            "node {node} has multiple inputs but only 1 is expected"
        )),
    }
}

fn has_output<'a>(flat: &FlatGraph<'a>, node: Vertex<'a>) -> anyhow::Result<bool> {
    let bindings = flat
        .bindings_reverse
        .get(&node)
        .with_context(|| format!("node {node} doesnt have bindings"))?;

    Ok(flat
        .edges
        .iter()
        .any(|(from, _)| bindings.iter().any(|b| b == from)))
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(graph),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub fn parse<'a>(graph: &'a Graph) -> anyhow::Result<Code<'a>> {
    record_trace!(graph);

    let flat = graph.try_visit(FlatGraph::default(), Visitor)?;
    let partial = flat
        .vertexes
        .iter()
        .try_fold(PartialCode::default(), |mut partial, &node| {
            let context = resolve_context(&flat, node)?;

            match context {
                NodeContext::Input => {
                    let old = partial.input.replace(Input);
                    if old.is_some() {
                        return Err(anyhow!("graph has multiple input nodes"));
                    }
                }
                NodeContext::Output => {
                    let from = resolve_from(&flat, node)?;
                    let old = partial.output.replace(Output { from });
                    if old.is_some() {
                        return Err(anyhow!("graph has multiple output nodes"));
                    }
                }
                NodeContext::Compress => {
                    let from = resolve_froms(&flat, node)?.collect();
                    let output = has_output(&flat, node)?;
                    partial.nodes.insert(node, Node::Compress { from, output });
                }
                NodeContext::TextModel => {
                    let from = resolve_from(&flat, node)?;
                    let output = has_output(&flat, node)?;
                    partial.nodes.insert(node, Node::TextModel { from, output });
                }
                NodeContext::TTIModel => {
                    let from = resolve_from(&flat, node)?;
                    let output = has_output(&flat, node)?;
                    partial.nodes.insert(node, Node::TTIModel { from, output });
                }
                NodeContext::TTSModel => {
                    let from = resolve_from(&flat, node)?;
                    let output = has_output(&flat, node)?;
                    partial.nodes.insert(node, Node::TTSModel { from, output });
                }
            }

            anyhow::Ok(partial)
        })?;

    match partial {
        PartialCode {
            input: Some(_),
            nodes,
            output,
        } => Ok(Code { nodes, output }),
        PartialCode { input: None, .. } => Err(anyhow!("input is missing")),
    }
}
