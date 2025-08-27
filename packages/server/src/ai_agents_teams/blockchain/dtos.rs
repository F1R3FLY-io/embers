use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, StructuralConvert, Deserialize)]
#[convert(into(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
    pub graph_ast: Option<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::Name), from(models::Name))]
pub enum Name {
    Wildcard,
    VVar(String),
    GVar(String),
    QuoteGraph(Box<Graph>),
    QuoteVertex(Box<Vertex>),
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::Vertex), from(models::Vertex))]
pub struct Vertex {
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::Binding), from(models::Binding))]
pub struct Binding {
    pub graph: Box<Graph>,
    pub var: String,
    pub vertex: Vertex,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GVertex), from(models::GVertex))]
pub struct GVertex {
    pub graph: Box<Graph>,
    pub vertex: Vertex,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GVar), from(models::GVar))]
pub struct GVar {
    pub graph: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GEdgeAnon), from(models::GEdgeAnon))]
pub struct GEdgeAnon {
    pub binding_1: Binding,
    pub binding_2: Binding,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GEdgeNamed), from(models::GEdgeNamed))]
pub struct GEdgeNamed {
    pub binding_1: Binding,
    pub binding_2: Binding,
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GRuleAnon), from(models::GRuleAnon))]
pub struct GRuleAnon {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GRuleNamed), from(models::GRuleNamed))]
pub struct GRuleNamed {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GSubgraph), from(models::GSubgraph))]
pub struct GSubgraph {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::GTensor), from(models::GTensor))]
pub struct GTensor {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Serialize, Deserialize)]
#[convert(into(models::Graph), from(models::Graph))]
pub enum Graph {
    Nil,
    Vertex(GVertex),
    Var(GVar),
    Nominate(Binding),
    EdgeAnon(GEdgeAnon),
    EdgeNamed(GEdgeNamed),
    RuleAnon(GRuleAnon),
    RuleNamed(GRuleNamed),
    Subgraph(GSubgraph),
    Tensor(GTensor),
}

impl From<Box<Graph>> for Box<models::Graph> {
    fn from(value: Box<Graph>) -> Self {
        Self::new((*value).into())
    }
}

impl From<Box<models::Graph>> for Box<Graph> {
    fn from(value: Box<models::Graph>) -> Self {
        Self::new((*value).into())
    }
}

impl From<Box<Vertex>> for Box<models::Vertex> {
    fn from(value: Box<Vertex>) -> Self {
        Self::new((*value).into())
    }
}

impl From<Box<models::Vertex>> for Box<Vertex> {
    fn from(value: Box<models::Vertex>) -> Self {
        Self::new((*value).into())
    }
}
