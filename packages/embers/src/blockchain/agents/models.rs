use serde::Deserialize;
use structural_convert::StructuralConvert;

use crate::blockchain::common::DateTime;
use crate::domain::agents::models;

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agents))]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentHeader))]
pub struct AgentHeader {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub last_deploy: Option<DateTime>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    pub last_deploy: Option<DateTime>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    // Reverse the Rholang string escaping the tuplespace preserves — otherwise
    // code like `@Nil!("foo")` reads back as `@Nil!(\"foo\")`. This is also the
    // form the agent-deploy path re-embeds as a Rholang term, so it must be
    // unescaped here.
    #[serde(
        default,
        deserialize_with = "crate::blockchain::common::deserialize_unescaped_opt"
    )]
    pub code: Option<String>,
}
