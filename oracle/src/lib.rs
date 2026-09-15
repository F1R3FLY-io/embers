//! A parse oracle for embers' generated Rholang.
//!
//! Everything embers deploys is Rholang it renders from Askama templates. This
//! crate feeds that rendered output through the **real, current** rholang-rs
//! tree-sitter parser and asserts it is accepted — the same parser the node
//! uses — so a template that renders syntactically invalid Rholang (for example
//! the historical `agent` soft-keyword collision) fails here instead of at
//! deploy time on a live node.
//!
//! It is deliberately detached from the embers Cargo workspace (see Cargo.toml):
//! it depends on a local rholang-rs checkout that must not enter embers' own
//! dependency graph.

use rholang_parser::RholangParser;
use validated::Validated;

/// Parse `code` with the real Rholang parser. Returns `Ok(())` when the parser
/// accepts it, or `Err` carrying a rendering of the accumulated parse errors.
///
/// A fresh [`RholangParser`] is constructed per call: the parser owns a
/// tree-sitter parse context that is not shared across threads, and the corpus
/// is small enough that per-call construction is not a bottleneck.
pub fn parse_result(code: &str) -> Result<(), String> {
    let parser = RholangParser::new();
    match parser.parse(code) {
        Validated::Good(_) => Ok(()),
        Validated::Fail(failures) => Err(format!("{failures:?}")),
    }
}

/// Panic with a labelled, readable message if `code` does not parse.
pub fn assert_parses(code: &str, label: &str) {
    if let Err(err) = parse_result(code) {
        panic!("{label} does not parse under the Rholang parser:\n{err}");
    }
}
