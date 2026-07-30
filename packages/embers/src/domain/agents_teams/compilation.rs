mod graphl_parsing;
mod rendering;

// Only the deploy tests name vertices directly; re-exporting unconditionally left the
// binary build warning about an unused import.
#[cfg(test)]
pub use graphl_parsing::Vertex;
pub use graphl_parsing::{AgentRef, Code, Node, parse, references_agent};
pub use rendering::{AgentCodes, render};
