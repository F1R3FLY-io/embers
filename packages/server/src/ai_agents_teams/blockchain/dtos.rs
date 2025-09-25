use serde::{Deserialize, de};
use structural_convert::StructuralConvert;

use crate::ai_agents_teams::models;

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
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Graph(models::Graph);

impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let graphl = String::deserialize(deserializer)?;
        models::Graph::new(graphl)
            .map(Self)
            .map_err(de::Error::custom)
    }
}

impl From<Graph> for models::Graph {
    fn from(value: Graph) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<Graph>,
}
