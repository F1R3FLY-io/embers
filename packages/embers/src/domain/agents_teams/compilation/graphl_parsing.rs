use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, anyhow};
use derive_more::{AsRef, Display, From};
use serde::Deserialize;

use crate::domain::agents_teams::models::Graph;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, AsRef, Display)]
pub struct Vertex<'a>(&'a str);

impl<'a> TryFrom<&'a graphl_parser::ast::Name> for Vertex<'a> {
    type Error = anyhow::Error;

    fn try_from(value: &'a graphl_parser::ast::Name) -> Result<Self, Self::Error> {
        match value {
            graphl_parser::ast::Name::Wildcard => {
                Err(anyhow!("Wildcar is not supported in vertex name"))
            }
            graphl_parser::ast::Name::VVar { value } => Ok(Vertex(value)),
            graphl_parser::ast::Name::GVar { value } => Ok(Vertex(value)),
            graphl_parser::ast::Name::QuoteGraph { .. } => {
                Err(anyhow!("QuoteGraph is not supported in vertex name"))
            }
            graphl_parser::ast::Name::QuoteVertex { .. } => {
                Err(anyhow!("QuoteVertex is not supported in vertex name"))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, AsRef, Display)]
pub struct Name<'a>(&'a str);

impl<'a> TryFrom<&'a graphl_parser::ast::Name> for Name<'a> {
    type Error = anyhow::Error;

    fn try_from(value: &'a graphl_parser::ast::Name) -> Result<Self, Self::Error> {
        match value {
            graphl_parser::ast::Name::Wildcard => Err(anyhow!("Wildcar is not supported in name")),
            graphl_parser::ast::Name::VVar { value } => Ok(Name(value)),
            graphl_parser::ast::Name::GVar { value } => Ok(Name(value)),
            graphl_parser::ast::Name::QuoteGraph { .. } => {
                Err(anyhow!("QuoteGraph is not supported in name"))
            }
            graphl_parser::ast::Name::QuoteVertex { .. } => {
                Err(anyhow!("QuoteVertex is not supported in name"))
            }
        }
    }
}

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
        let name = (&vertex.vertex.name).try_into()?;
        acc.vertexes.insert(name);
        Ok(acc)
    }

    fn visit_nominate(
        &self,
        mut acc: FlatGraph<'a>,
        binding: &'a graphl_parser::ast::Binding,
    ) -> Result<FlatGraph<'a>, anyhow::Error> {
        let name = (&binding.vertex.name).try_into()?;
        let binding = Name(&binding.var);

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
        let name = (&context.name).try_into()?;

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
    #[serde(rename = "input-node")]
    Input,
    #[serde(rename = "output-node")]
    Output,
    #[serde(rename = "compress")]
    Compress,
    #[serde(rename = "text-model")]
    TextModel,
    #[serde(rename = "tti-model")]
    TTIModel,
    #[serde(rename = "tts-model")]
    TTSModel,
    /// A user-defined agent, referenced by id and version. The agent's Rholang
    /// is fetched from agents storage at deploy time and spliced into the team.
    /// `agent_name` is the label the author gave it; it names this agent's entry
    /// when a downstream node joins several inputs into a map.
    #[serde(rename = "agent", rename_all = "camelCase")]
    Agent {
        agent_id: String,
        agent_version: String,
        agent_name: String,
    },
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
    Compress {
        from: Vec<Vertex<'a>>,
        output: bool,
    },
    TextModel {
        from: Vertex<'a>,
        output: bool,
    },
    TTIModel {
        from: Vertex<'a>,
        output: bool,
    },
    TTSModel {
        from: Vertex<'a>,
        output: bool,
    },
    /// Like `Compress`, an agent may take several incoming edges: they are
    /// joined and delivered as a map. A single edge is delivered as the value.
    Agent {
        agent_id: String,
        agent_version: String,
        agent_name: String,
        from: Vec<Vertex<'a>>,
        output: bool,
    },
}

impl Node<'_> {
    pub const fn output(&self) -> bool {
        match self {
            Node::Compress { output, .. } => *output,
            Node::TextModel { output, .. } => *output,
            Node::TTIModel { output, .. } => *output,
            Node::TTSModel { output, .. } => *output,
            Node::Agent { output, .. } => *output,
        }
    }
}

/// Identifies a specific version of a user-defined agent referenced by a team.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AgentRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct Code<'a> {
    pub nodes: BTreeMap<Vertex<'a>, Node<'a>>,
    pub output: Option<Output<'a>>,
}

impl Code<'_> {
    /// Every agent version this team references. Deduplicated, so an agent used
    /// by two nodes is fetched once.
    /// The author-facing name of a referenced agent, when the graph records one.
    ///
    /// `AgentRef` stays pure identity -- it keys the resolved-code map, so folding a
    /// name into it would split one agent into two entries if a graph ever carried
    /// differing names for the same id. The name is looked up separately instead, so
    /// an error can say "Refine" rather than a pair of uuids.
    pub fn agent_name(&self, wanted: &AgentRef) -> Option<&str> {
        self.nodes.values().find_map(|node| match node {
            Node::Agent {
                agent_id,
                agent_version,
                agent_name,
                ..
            } if *agent_id == wanted.id && *agent_version == wanted.version => {
                Some(agent_name.as_str())
            }
            _ => None,
        })
    }

    pub fn agent_refs(&self) -> BTreeSet<AgentRef> {
        self.nodes
            .values()
            .filter_map(|node| match node {
                Node::Agent {
                    agent_id,
                    agent_version,
                    ..
                } => Some(AgentRef {
                    id: agent_id.clone(),
                    version: agent_version.clone(),
                }),
                _ => None,
            })
            .collect()
    }
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
        (None, _) => return Err(missing_context_error(flat, node)),
        (Some(_), Some(_)) => return Err(anyhow!("node {node} has multiple contexts")),
    };

    serde_json::from_str(context).with_context(|| format!("node {node} has invalid context"))
}

/// Explains a vertex that edges refer to but nothing defines.
///
/// The bare identifier alone is close to useless to the person who has to act on it:
/// it names an internal id, in a process they are not looking at, for a node that by
/// definition has no label. Naming the neighbours the broken edges lead to, and
/// stating the remedy, is what makes the message actionable — the graph is repaired
/// simply by opening it in the editor and saving, because the editor drops edges
/// whose endpoints it cannot resolve.
///
/// See `docs/post-mortems/AGENT_TEAM_EDITOR_GRAPH_CORRUPTION.md` in embers-frontend.
fn missing_context_error(flat: &FlatGraph<'_>, node: Vertex<'_>) -> anyhow::Error {
    let names = flat.bindings_reverse.get(&node);

    // Every edge with this vertex at either end, described by the *other* end, which
    // — unlike this one — usually does have a context to name it by.
    let mut neighbours: Vec<String> = flat
        .edges
        .iter()
        .filter_map(|(from, to)| {
            let names = names?;
            match (names.contains(from), names.contains(to)) {
                (true, false) => Some(format!("-> {}", describe_vertex(flat, *to))),
                (false, true) => Some(format!("{} ->", describe_vertex(flat, *from))),
                _ => None,
            }
        })
        .collect();
    neighbours.sort();
    neighbours.dedup();

    if neighbours.is_empty() {
        return anyhow!(
            "node {node} is missing context: the graph refers to it but never defines it"
        );
    }

    anyhow!(
        "node {node} is missing context: {} edge(s) refer to it but the graph never \
         defines it ({}). This is what a node deleted without its edges leaves behind. \
         Open the team in the editor and save it — that drops the stale connections and \
         produces a graph that deploys.",
        neighbours.len(),
        neighbours.join(", "),
    )
}

/// A vertex named the way its author would recognise it: an agent's own name where
/// there is one, else the node kind, else the raw identifier.
fn describe_vertex(flat: &FlatGraph<'_>, name: Name<'_>) -> String {
    let Some(context) = flat.contexts.get(&name) else {
        return name.to_string();
    };

    match serde_json::from_str::<NodeContext>(context) {
        Ok(NodeContext::Agent { agent_name, .. }) => agent_name,
        Ok(NodeContext::Input) => "Input".to_owned(),
        Ok(NodeContext::Output) => "Output".to_owned(),
        Ok(NodeContext::Compress) => "Compress".to_owned(),
        Ok(NodeContext::TextModel) => "Text model".to_owned(),
        Ok(NodeContext::TTIModel) => "TTI model".to_owned(),
        Ok(NodeContext::TTSModel) => "TTS model".to_owned(),
        Err(_) => name.to_string(),
    }
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
/// Whether a graph names the given agent anywhere.
///
/// Deliberately does **not** go through `parse`. A team whose graph is semantically
/// broken -- a dangling edge, say -- still uses whatever agents its contexts name,
/// and a team that cannot be scanned must not silently drop out of a deletion
/// warning: reporting "no teams affected" when a team is affected is precisely the
/// silent failure this guards against. Collecting contexts succeeds on graphs that
/// `parse` rejects, so the answer stays available.
///
/// Precise rather than a substring search: each context is deserialized, so an
/// identifier appearing in some other field cannot masquerade as a reference.
pub fn references_agent(graph: &Graph, agent_id: &str) -> anyhow::Result<bool> {
    let flat = graph.try_visit(FlatGraph::default(), Visitor)?;

    Ok(flat.contexts.values().any(|context| {
        matches!(
            serde_json::from_str::<NodeContext>(context),
            Ok(NodeContext::Agent { agent_id: ref id, .. }) if id == agent_id
        )
    }))
}

pub fn parse(graph: &Graph) -> anyhow::Result<Code<'_>> {
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
                NodeContext::Agent {
                    agent_id,
                    agent_version,
                    agent_name,
                } => {
                    let from = resolve_froms(&flat, node)?.collect();
                    let output = has_output(&flat, node)?;
                    partial.nodes.insert(
                        node,
                        Node::Agent {
                            agent_id,
                            agent_version,
                            agent_name,
                            from,
                            output,
                        },
                    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agents_teams::models::Graph;

    /// The graphl an agent node produces, in the shape the editor emits: three
    /// context-tagged vertices plus the edges wiring them together.
    fn agent_team_graphl() -> String {
        let agent_context = r#"{\"type\":\"agent\",\"agentId\":\"019f-abc\",\"agentName\":\"Summarize\",\"agentVersion\":\"019f-def\"}"#;
        format!(
            r#"0 * {{context "{{\"type\":\"input-node\"}}" for input1 in let input1 = < input1 > in < input1 > | {{context "{agent_context}" for model1 in let model1 = < model1 > in < model1 > | {{context "{{\"type\":\"output-node\"}}" for output1 in let output1 = < output1 > in < output1 > | 0}}}}}} * {{(let input1 = < input1 > in 0, let model1 = < model1 > in 0) * {{(let model1 = < model1 > in 0, let output1 = < output1 > in 0) * 0}}}}"#
        )
    }

    #[test]
    fn parses_an_agent_node_from_editor_graphl() {
        let graph = Graph::new(agent_team_graphl()).expect("graphl should parse");
        let code = parse(&graph).expect("an agent node should compile");

        let agent = code
            .nodes
            .values()
            .find_map(|node| match node {
                Node::Agent {
                    agent_id,
                    agent_version,
                    agent_name,
                    from,
                    output,
                } => Some((agent_id, agent_version, agent_name, from.len(), *output)),
                _ => None,
            })
            .expect("the graph should yield an agent node");

        assert_eq!(agent.0, "019f-abc", "agent id should survive the context");
        assert_eq!(agent.1, "019f-def", "agent version should survive");
        assert_eq!(agent.2, "Summarize", "agent name should survive");
        assert_eq!(agent.3, 1, "this agent has a single incoming edge");
        assert!(agent.4, "the agent feeds the output node");
    }

    #[test]
    fn collects_agent_references_for_resolution() {
        let graph = Graph::new(agent_team_graphl()).expect("graphl should parse");
        let code = parse(&graph).expect("an agent node should compile");

        let refs = code.agent_refs();
        assert_eq!(refs.len(), 1);
        let agent_ref = refs.iter().next().expect("one reference");
        assert_eq!(agent_ref.id, "019f-abc");
        assert_eq!(agent_ref.version, "019f-def");
    }

    // -------------------------------------------------------
    // fan-out / fan-in: the red-team topology, end to end
    // through the real graphl parser
    // -------------------------------------------------------

    /// Assembles graphl in the shape the editor emits: vertices nested inside
    /// their contexts, then a right-nested tensor chain of anonymous edges.
    fn graphl(nodes: &[(&str, String)], edges: &[(&str, &str)]) -> String {
        let mut body = "0".to_owned();
        for (name, json) in nodes.iter().rev() {
            let escaped = json.replace('"', "\\\"");
            body = format!(
                "{{context \"{escaped}\" for {name} in let {name} = < {name} > in < {name} > | {body}}}"
            );
        }

        let mut edge_chain = "0".to_owned();
        for (from, to) in edges.iter().rev() {
            edge_chain = format!(
                "{{(let {from} = < {from} > in 0, let {to} = < {to} > in 0) * {edge_chain}}}"
            );
        }

        format!("0 * {body} * {edge_chain}")
    }

    fn agent_context(name: &str) -> String {
        format!(
            r#"{{"type":"agent","agentId":"{name}-id","agentName":"{name}","agentVersion":"v1"}}"#
        )
    }

    /// Input feeds every reasoning node; the hypothesis fans out to the three
    /// critics and the refiner; the critics fan back into the refiner.
    fn red_team_graphl() -> String {
        graphl(
            &[
                ("aIn", r#"{"type":"input-node"}"#.to_owned()),
                ("aHyp", agent_context("Hypothesize")),
                ("aLog", agent_context("LogicCritic")),
                ("aEvi", agent_context("EvidenceCritic")),
                ("aFea", agent_context("FeasibilityCritic")),
                ("aRef", agent_context("Refine")),
                ("aCon", agent_context("Condense")),
                ("aOut", r#"{"type":"output-node"}"#.to_owned()),
            ],
            &[
                ("aIn", "aHyp"),
                ("aIn", "aLog"),
                ("aIn", "aEvi"),
                ("aIn", "aFea"),
                ("aIn", "aRef"),
                ("aIn", "aCon"),
                ("aHyp", "aLog"),
                ("aHyp", "aEvi"),
                ("aHyp", "aFea"),
                ("aHyp", "aRef"),
                ("aLog", "aRef"),
                ("aEvi", "aRef"),
                ("aFea", "aRef"),
                ("aRef", "aCon"),
                ("aCon", "aOut"),
            ],
        )
    }

    fn agent_arity(code: &Code<'_>, name: &str) -> usize {
        code.nodes
            .values()
            .find_map(|node| match node {
                Node::Agent {
                    agent_name, from, ..
                } if agent_name == name => Some(from.len()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no agent node named {name}"))
    }

    #[test]
    fn parses_fan_out_and_fan_in_arities() {
        let graph = Graph::new(red_team_graphl()).expect("red-team graphl should parse");
        let code = parse(&graph).expect("the red-team graph should compile");

        assert_eq!(
            code.nodes.len(),
            6,
            "six agent nodes, input and output aside"
        );
        assert_eq!(agent_arity(&code, "Hypothesize"), 1, "input only");
        assert_eq!(agent_arity(&code, "LogicCritic"), 2, "input and hypothesis");
        assert_eq!(agent_arity(&code, "EvidenceCritic"), 2);
        assert_eq!(agent_arity(&code, "FeasibilityCritic"), 2);
        assert_eq!(
            agent_arity(&code, "Refine"),
            5,
            "input, hypothesis and all three critiques"
        );
        assert_eq!(agent_arity(&code, "Condense"), 2, "input and refinement");
    }

    #[test]
    fn fan_out_reaches_every_target() {
        let graph = Graph::new(red_team_graphl()).expect("red-team graphl should parse");
        let code = parse(&graph).expect("the red-team graph should compile");

        // The hypothesis vertex must appear as an input of all four of its
        // targets; losing an edge here is exactly the failure a fan-out bug
        // would produce.
        let readers = code
            .nodes
            .values()
            .filter(|node| match node {
                Node::Agent {
                    agent_name, from, ..
                } => {
                    agent_name != "Hypothesize"
                        && from.iter().any(|vertex| *vertex.as_ref() == "aHyp")
                }
                _ => false,
            })
            .count();
        assert_eq!(
            readers, 4,
            "the hypothesis feeds three critics and the refiner"
        );
    }

    #[test]
    fn every_referenced_agent_version_is_collected_once() {
        let graph = Graph::new(red_team_graphl()).expect("red-team graphl should parse");
        let code = parse(&graph).expect("the red-team graph should compile");

        let refs = code.agent_refs();
        assert_eq!(refs.len(), 6, "six distinct agents, each fetched once");
        assert!(
            refs.iter().all(|agent_ref| agent_ref.version == "v1"),
            "versions should survive the context round-trip: {refs:?}"
        );
    }

    #[test]
    fn a_graph_without_agents_references_none() {
        let graphl = r#"0 * {context "{\"type\":\"input-node\"}" for input1 in let input1 = < input1 > in < input1 > | {context "{\"type\":\"text-model\"}" for model1 in let model1 = < model1 > in < model1 > | {context "{\"type\":\"output-node\"}" for output1 in let output1 = < output1 > in < output1 > | 0}}} * {(let input1 = < input1 > in 0, let model1 = < model1 > in 0) * {(let model1 = < model1 > in 0, let output1 = < output1 > in 0) * 0}}"#;
        let graph = Graph::new(graphl.to_owned()).expect("graphl should parse");
        let code = parse(&graph).expect("a built-in only graph should compile");

        assert!(code.agent_refs().is_empty());
    }

    #[test]
    fn a_dangling_edge_is_reported_with_the_neighbours_it_connects() {
        // What a node deleted without its edges leaves behind. The identifier alone
        // tells the author nothing -- the node has no context, so it has no label --
        // so the message has to describe the edges by their *other* end.
        let graphl = graphl(
            &[
                ("aIn", r#"{"type":"input-node"}"#.to_owned()),
                ("aCon", agent_context("Condense")),
                ("aOut", r#"{"type":"output-node"}"#.to_owned()),
            ],
            &[("aIn", "aCon"), ("aCon", "aGhost"), ("aGhost", "aOut")],
        );

        let graph = Graph::new(graphl).expect("graphl should parse");
        let error = parse(&graph).expect_err("a dangling edge must not compile");
        let message = format!("{error:#}");

        assert!(message.contains("is missing context"), "{message}");
        // The neighbours, named as the author knows them rather than by id.
        assert!(message.contains("Condense ->"), "{message}");
        assert!(message.contains("-> Output"), "{message}");
        // And the remedy, since the editor repairs this on load-and-save.
        assert!(message.contains("save"), "{message}");
    }

    #[test]
    fn a_well_formed_graph_reports_nothing() {
        // Guards the diagnostic against firing on healthy input.
        let graph = Graph::new(red_team_graphl()).expect("graphl should parse");

        assert!(parse(&graph).is_ok());
    }
}
