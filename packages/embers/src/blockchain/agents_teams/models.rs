use derive_more::Into;
use serde::{Deserialize, Serialize, de};
use structural_convert::StructuralConvert;

use crate::blockchain::common::{DateTime, Hex, Uri};
use crate::domain::agents_teams::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeams))]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeamHeader))]
pub struct AgentsTeamHeader {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub last_deploy: Option<DateTime>,
    pub uri: Option<Uri>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Into)]
pub struct Graph(models::Graph);

impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        // The Rholang tuplespace stores string content without unescaping
        // escape sequences. Undo the escape_rho_string() level that was
        // added when embedding the graphl string in Rholang source code.
        // Order matters — reverse of escape_rho_string which does
        // .replace('\\', "\\\\").replace('"', "\\\""):
        let graphl = raw.replace("\\\"", "\"").replace("\\\\", "\\");
        models::Graph::new(graphl.clone()).map(Self).map_err(|e| {
            tracing::error!(
                error = %e,
                graphl_len = graphl.len(),
                graphl_prefix = %&graphl[..graphl.len().min(200)],
                raw_prefix = %&raw[..raw.len().min(200)],
                "DIAG: graphl_parser::parse_to_ast failed during Graph deserialization"
            );
            de::Error::custom(e)
        })
    }
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub last_deploy: Option<DateTime>,
    pub uri: Option<Uri>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Graph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireskyCredentials {
    pub uri: String,
    pub pds_url: String,
    pub email: String,
    pub token: String,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::EncryptedMsg))]
pub struct EncryptedMsg {
    pub ciphertext: Hex,
    pub nonce: Hex,
}

#[cfg(test)]
mod tests {
    use crate::domain::agents_teams::models::Graph;

    /// Simulate `escape_rho_string` from `firefly_client::rendering`.
    fn escape_rho_string(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Simulate the unescape performed by the Graph deserializer.
    /// This reverses `escape_rho_string` — the same operation the
    /// deserializer applies to strings returned from the tuplespace.
    fn undo_escape_rho_string(s: &str) -> String {
        s.replace("\\\"", "\"").replace("\\\\", "\\")
    }

    #[test]
    fn graphl_round_trip_simple() {
        // A minimal graphl context annotation (no JSON, no escapes).
        let original = r#"context "foo" for f in 0"#;
        let ast = Graph::new(original.to_owned()).expect("original should parse");

        let graphl = ast.graphl();
        let re_parsed = Graph::new(graphl.clone()).expect("serialized form should re-parse");
        let re_serialized = re_parsed.graphl();
        assert_eq!(
            graphl, re_serialized,
            "graphl round-trip must be idempotent"
        );
    }

    #[test]
    fn graphl_round_trip_through_rho_escaping() {
        // 1. Start with a graphl string containing a context annotation with JSON.
        //    The graphl format uses `\"` inside string literals, so the raw graphl
        //    looks like: context "{\"type\":\"deploy-container\"}" for f in 0
        let original = r#"context "{\"type\":\"deploy-container\"}" for f in 0"#;
        let ast = Graph::new(original.to_owned()).expect("original should parse");

        // 2. Serialize back to graphl (simulates Graph::graphl()).
        let graphl = ast.graphl();

        // 3. Apply escape_rho_string (simulates Rholang template embedding).
        //    This is what gets stored in the tuplespace — Rholang does NOT
        //    unescape, so the stored string retains the extra escaping.
        let stored = escape_rho_string(&graphl);

        // 4. The stored string should NOT parse directly (it has extra escaping).
        assert!(
            Graph::new(stored.clone()).is_err(),
            "escaped string should not parse without unescaping"
        );

        // 5. Apply undo_escape_rho_string (what the deserializer does).
        let unescaped = undo_escape_rho_string(&stored);

        // 6. Verify the unescaped string equals the original graphl.
        assert_eq!(
            graphl, unescaped,
            "undo_escape_rho_string should recover the original graphl"
        );

        // 7. Re-parse the unescaped string.
        let re_parsed = Graph::new(unescaped).expect("unescaped string should parse");

        // 8. Verify AST equality via re-serialization.
        let re_serialized = re_parsed.graphl();
        assert_eq!(
            graphl, re_serialized,
            "ASTs should produce identical graphl"
        );
    }

    #[test]
    fn graphl_round_trip_complex_json_context() {
        // A more complex context annotation with nested JSON.
        let original =
            r#"context "{\"agents\":[{\"name\":\"bot\",\"model\":\"gpt-4\"}]}" for x in 0"#;
        let ast = Graph::new(original.to_owned()).expect("original should parse");
        let graphl = ast.graphl();

        // Stored in tuplespace with extra escaping (Rholang does NOT unescape).
        let stored = escape_rho_string(&graphl);
        let unescaped = undo_escape_rho_string(&stored);

        assert_eq!(graphl, unescaped);

        let re_parsed = Graph::new(unescaped).expect("unescaped string should parse");
        assert_eq!(graphl, re_parsed.graphl());
    }

    #[test]
    fn graphl_round_trip_with_vertex_and_edge() {
        // A graph with vertices, bindings, edges, AND a context annotation.
        let original = r#"< a > | { context "{\"k\":\"v\"}" for a in 0 }"#;
        let ast = Graph::new(original.to_owned()).expect("original should parse");
        let graphl = ast.graphl();

        // Stored in tuplespace with extra escaping (Rholang does NOT unescape).
        let stored = escape_rho_string(&graphl);
        let unescaped = undo_escape_rho_string(&stored);

        assert_eq!(graphl, unescaped);
        let re_parsed = Graph::new(unescaped).expect("unescaped string should parse");
        assert_eq!(graphl, re_parsed.graphl());
    }

    #[test]
    fn graphl_parser_is_thread_safe_under_mutex() {
        // The C parser uses global state; this test verifies the mutex
        // serializes concurrent access correctly.
        let inputs = [
            r#"context "foo" for f in 0"#,
            r#"context "{\"type\":\"deploy-container\"}" for x in 0"#,
            r#"< a > | { context "{\"k\":\"v\"}" for a in 0 }"#,
            r#"context "{\"agents\":[{\"name\":\"bot\"}]}" for y in 0"#,
        ];

        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for _ in 0..4 {
                for input in &inputs {
                    handles.push(s.spawn(|| {
                        let ast = Graph::new(input.to_string()).expect("parse should succeed");
                        let graphl = ast.graphl();
                        let re_parsed =
                            Graph::new(graphl.clone()).expect("re-parse should succeed");
                        assert_eq!(graphl, re_parsed.graphl());
                    }));
                }
            }
            for handle in handles {
                handle.join().expect("thread should not panic");
            }
        });
    }
}
