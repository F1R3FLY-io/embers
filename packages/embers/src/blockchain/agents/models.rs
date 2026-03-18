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
    #[serde(default)]
    pub last_deploy: Option<DateTime>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub shard: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub created_at: DateTime,
    #[serde(default)]
    pub last_deploy: Option<DateTime>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub shard: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}
