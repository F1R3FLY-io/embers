//! Grammar-directed `GraphL` properties.
//!
//! The `GraphL` parser is a Flex/Bison C FFI with process-wide mutable state
//! behind [`crate::domain::agents_teams::models::Graph`]'s mutex, so these
//! strategies only ever emit grammar-shaped text (the editor's own emission
//! shape, mirrored from the `graphl_parsing.rs` test builders) or bounded
//! whole-token mutations of it — never raw bytes, which could abort the C
//! layer and kill the test process.

use proptest::prelude::*;

use crate::domain::agents_teams::compilation::{Node, parse};
use crate::domain::agents_teams::models::Graph;

/// A middle (non input/output) node of a generated team graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MiddleKind {
    Compress,
    TextModel,
    TtiModel,
    TtsModel,
    Agent,
}

impl MiddleKind {
    const fn from_choice(choice: u8) -> Self {
        match choice % 5 {
            0 => Self::Compress,
            1 => Self::TextModel,
            2 => Self::TtiModel,
            3 => Self::TtsModel,
            _ => Self::Agent,
        }
    }

    /// Whether the node kind accepts several incoming edges (joined into a
    /// map) or exactly one.
    const fn multi_input(self) -> bool {
        matches!(self, Self::Compress | Self::Agent)
    }
}

/// A fully resolved specification: every middle knows its kind and its
/// upstream vertices (`0` is the input node, `1 + j` is middle `j`).
#[derive(Debug, Clone)]
struct GraphSpec {
    middles: Vec<(MiddleKind, Vec<usize>)>,
    output_from: Option<usize>,
}

fn vertex_name(index: usize) -> String {
    if index == 0 {
        "aIn".to_owned()
    } else {
        format!("aM{}", index - 1)
    }
}

/// Derives a valid spec from raw choice material, so any raw vector is a
/// valid graph and shrinking stays structural.
fn resolve_spec(raw_middles: &[(u8, u8)], output_choice: u8, with_output: bool) -> GraphSpec {
    let middles = raw_middles
        .iter()
        .enumerate()
        .map(|(index, &(kind_choice, source_mask))| {
            let kind = MiddleKind::from_choice(kind_choice);
            let available = index + 1;
            let sources = if kind.multi_input() {
                let mut sources: Vec<usize> = (0..available.min(8))
                    .filter(|source| source_mask & (1 << source) != 0)
                    .collect();
                if sources.is_empty() {
                    sources.push(0);
                }
                sources
            } else {
                vec![usize::from(source_mask) % available]
            };
            (kind, sources)
        })
        .collect::<Vec<_>>();
    let output_from =
        with_output.then(|| usize::from(output_choice) % (raw_middles.len() + 1));
    GraphSpec {
        middles,
        output_from,
    }
}

/// Assembles `GraphL` in the shape the editor emits — vertices nested inside
/// their contexts, then a right-nested tensor chain of anonymous edges —
/// mirroring the `graphl()` helper in the `graphl_parsing.rs` test module.
fn graphl_text(spec: &GraphSpec) -> String {
    let mut nodes: Vec<(String, String)> =
        vec![(vertex_name(0), r#"{"type":"input-node"}"#.to_owned())];
    for (index, (kind, _)) in spec.middles.iter().enumerate() {
        let context = match kind {
            MiddleKind::Compress => r#"{"type":"compress"}"#.to_owned(),
            MiddleKind::TextModel => r#"{"type":"text-model"}"#.to_owned(),
            MiddleKind::TtiModel => r#"{"type":"tti-model"}"#.to_owned(),
            MiddleKind::TtsModel => r#"{"type":"tts-model"}"#.to_owned(),
            MiddleKind::Agent => format!(
                r#"{{"type":"agent","agentId":"agent{index}-id","agentName":"Agent{index}","agentVersion":"v1"}}"#
            ),
        };
        nodes.push((vertex_name(index + 1), context));
    }
    let mut edges: Vec<(String, String)> = Vec::new();
    for (index, (_, sources)) in spec.middles.iter().enumerate() {
        for &source in sources {
            edges.push((vertex_name(source), vertex_name(index + 1)));
        }
    }
    if let Some(source) = spec.output_from {
        nodes.push(("aOut".to_owned(), r#"{"type":"output-node"}"#.to_owned()));
        edges.push((vertex_name(source), "aOut".to_owned()));
    }

    let mut body = "0".to_owned();
    for (name, json) in nodes.iter().rev() {
        let escaped = json.replace('"', "\\\"");
        body = format!(
            "{{context \"{escaped}\" for {name} in let {name} = < {name} > in < {name} > | {body}}}"
        );
    }
    let mut edge_chain = "0".to_owned();
    for (from, to) in edges.iter().rev() {
        edge_chain =
            format!("{{(let {from} = < {from} > in 0, let {to} = < {to} > in 0) * {edge_chain}}}");
    }
    format!("0 * {body} * {edge_chain}")
}

fn arb_graph_spec() -> impl Strategy<Value = GraphSpec> {
    (
        proptest::collection::vec((any::<u8>(), any::<u8>()), 0..=3),
        any::<u8>(),
        any::<bool>(),
    )
        .prop_map(|(raw_middles, output_choice, with_output)| {
            resolve_spec(&raw_middles, output_choice, with_output)
        })
}

/// One whole-token structural mutation.
#[derive(Debug, Clone, Copy)]
enum TokenMutation {
    Drop(u16),
    Duplicate(u16),
    SwapAdjacent(u16),
}

fn arb_mutations() -> impl Strategy<Value = Vec<TokenMutation>> {
    proptest::collection::vec(
        prop_oneof![
            any::<u16>().prop_map(TokenMutation::Drop),
            any::<u16>().prop_map(TokenMutation::Duplicate),
            any::<u16>().prop_map(TokenMutation::SwapAdjacent),
        ],
        1..=3,
    )
}

fn mutate_tokens(text: &str, mutations: &[TokenMutation]) -> String {
    let mut tokens: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    for mutation in mutations {
        if tokens.is_empty() {
            break;
        }
        match *mutation {
            TokenMutation::Drop(position) => {
                let index = usize::from(position) % tokens.len();
                tokens.remove(index);
            }
            TokenMutation::Duplicate(position) => {
                let index = usize::from(position) % tokens.len();
                let token = tokens[index].clone();
                tokens.insert(index, token);
            }
            TokenMutation::SwapAdjacent(position) => {
                if tokens.len() >= 2 {
                    let index = usize::from(position) % (tokens.len() - 1);
                    tokens.swap(index, index + 1);
                }
            }
        }
    }
    tokens.join(" ")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    /// Parsing editor-shaped GraphL and re-serializing it reaches a fixed
    /// point after the first normalizing round trip.
    #[test]
    fn fv_inv_g1_graphl_round_trip_fixed_point(spec in arb_graph_spec()) {
        let text = graphl_text(&spec);
        let parsed = Graph::new(text.clone());
        let graph = match parsed {
            Ok(graph) => graph,
            Err(error) => return Err(TestCaseError::fail(format!(
                "generated GraphL must parse: {error:?}\n{text}"
            ))),
        };
        let first = graph.graphl();
        let reparsed = match Graph::new(first.clone()) {
            Ok(graph) => graph,
            Err(error) => return Err(TestCaseError::fail(format!(
                "serialized GraphL must re-parse: {error:?}\n{first}"
            ))),
        };
        let second = reparsed.graphl();
        prop_assert_eq!(first, second, "re-serialization must be a fixed point");
    }

    /// The semantic lowering agrees with the generating specification:
    /// every middle appears with its declared arity, the output edge is
    /// honored, and each referenced agent is collected exactly once.
    #[test]
    fn fv_inv_g2_graphl_lowering_matches_spec(spec in arb_graph_spec()) {
        let text = graphl_text(&spec);
        let graph = match Graph::new(text.clone()) {
            Ok(graph) => graph,
            Err(error) => return Err(TestCaseError::fail(format!(
                "generated GraphL must parse: {error:?}\n{text}"
            ))),
        };
        let code = match parse(&graph) {
            Ok(code) => code,
            Err(error) => return Err(TestCaseError::fail(format!(
                "well-formed graphs must lower: {error:#}\n{text}"
            ))),
        };

        prop_assert_eq!(code.nodes.len(), spec.middles.len());
        for (index, (kind, sources)) in spec.middles.iter().enumerate() {
            let name = vertex_name(index + 1);
            let node = code
                .nodes
                .iter()
                .find(|(vertex, _)| *vertex.as_ref() == name.as_str())
                .map(|(_, node)| node);
            let Some(node) = node else {
                return Err(TestCaseError::fail(format!("middle {name} missing")));
            };
            let arity = match node {
                Node::Compress { from, .. } | Node::Agent { from, .. } => from.len(),
                Node::TextModel { .. } | Node::TTIModel { .. } | Node::TTSModel { .. } => 1,
            };
            prop_assert_eq!(arity, sources.len(), "arity mismatch at {}", name);
            let expects_kind_match = match kind {
                MiddleKind::Compress => matches!(node, Node::Compress { .. }),
                MiddleKind::TextModel => matches!(node, Node::TextModel { .. }),
                MiddleKind::TtiModel => matches!(node, Node::TTIModel { .. }),
                MiddleKind::TtsModel => matches!(node, Node::TTSModel { .. }),
                MiddleKind::Agent => matches!(node, Node::Agent { .. }),
            };
            prop_assert!(expects_kind_match, "kind mismatch at {}", name);
        }
        prop_assert_eq!(code.output.is_some(), spec.output_from.is_some());
        let agent_count = spec
            .middles
            .iter()
            .filter(|(kind, _)| *kind == MiddleKind::Agent)
            .count();
        prop_assert_eq!(
            code.agent_refs().len(),
            agent_count,
            "each distinct agent is collected exactly once"
        );
    }

    /// Totality under bounded structural damage: dropping, duplicating, or
    /// swapping whole tokens of a valid graph yields either a parsed graph
    /// or a typed error — never a panic or a C-level abort. Mutants that
    /// still parse must also lower totally (Ok or typed Err).
    #[test]
    fn fv_inv_g3_graphl_token_mutations_total(
        spec in arb_graph_spec(),
        mutations in arb_mutations(),
    ) {
        let mutated = mutate_tokens(&graphl_text(&spec), &mutations);
        if let Ok(graph) = Graph::new(mutated) {
            // A typed lowering error (dangling edges, duplicate outputs, …)
            // is a valid outcome; only panics/aborts are failures.
            let _lowered = parse(&graph);
        }
    }
}
