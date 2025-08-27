use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::ai_agents_teams::models;
use crate::common::api::dtos::PreparedContract;

#[derive(Debug, Clone, Object)]
pub struct DeployDemoReq {
    pub name: String,
}

#[derive(Debug, Clone, Object)]
pub struct RunDemoReq {
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeams))]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeamHeader))]
pub struct AgentsTeamHeader {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::CreateAgentsTeamReq))]
pub struct CreateAgentsTeamReq {
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
    pub graph_ast: Option<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
    pub graph_ast: Option<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::CreateAgentsTeamResp))]
pub struct CreateAgentsTeamResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentsTeamReq = CreateAgentsTeamReq;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::SaveAgentsTeamResp))]
pub struct SaveAgentsTeamResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Default, Clone, Object)]
pub struct Unit {}

#[derive(Debug, Clone, Object)]
pub struct NVVar {
    value: String,
}

#[derive(Debug, Clone, Object)]
pub struct NGVar {
    value: String,
}

#[derive(Debug, Clone, Object)]
pub struct NQuoteGraph {
    value: Box<Graph>,
}

#[derive(Debug, Clone, Object)]
pub struct NQuoteVertex {
    value: Box<Vertex>,
}

#[derive(Debug, Clone, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum Name {
    Wildcard(Unit),
    VVar(NVVar),
    GVar(NGVar),
    QuoteGraph(NQuoteGraph),
    QuoteVertex(NQuoteVertex),
}

impl From<Name> for models::Name {
    fn from(value: Name) -> Self {
        match value {
            Name::Wildcard(_) => Self::Wildcard,
            Name::VVar(v) => Self::VVar(v.value),
            Name::GVar(v) => Self::GVar(v.value),
            Name::QuoteGraph(v) => Self::QuoteGraph(v.value.into()),
            Name::QuoteVertex(v) => Self::QuoteVertex(v.value.into()),
        }
    }
}

impl From<models::Name> for Name {
    fn from(value: models::Name) -> Self {
        match value {
            models::Name::Wildcard => Self::Wildcard(Default::default()),
            models::Name::VVar(v) => Self::VVar(NVVar { value: v }),
            models::Name::GVar(v) => Self::GVar(NGVar { value: v }),
            models::Name::QuoteGraph(v) => Self::QuoteGraph(NQuoteGraph { value: v.into() }),
            models::Name::QuoteVertex(v) => Self::QuoteVertex(NQuoteVertex { value: v.into() }),
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::Vertex), from(models::Vertex))]
pub struct Vertex {
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::Binding), from(models::Binding))]
pub struct Binding {
    pub graph: Box<Graph>,
    pub var: String,
    pub vertex: Vertex,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GVertex), from(models::GVertex))]
pub struct GVertex {
    pub graph: Box<Graph>,
    pub vertex: Vertex,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GVar), from(models::GVar))]
pub struct GVar {
    pub graph: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GEdgeAnon), from(models::GEdgeAnon))]
pub struct GEdgeAnon {
    pub binding_1: Binding,
    pub binding_2: Binding,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GEdgeNamed), from(models::GEdgeNamed))]
pub struct GEdgeNamed {
    pub binding_1: Binding,
    pub binding_2: Binding,
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GRuleAnon), from(models::GRuleAnon))]
pub struct GRuleAnon {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GRuleNamed), from(models::GRuleNamed))]
pub struct GRuleNamed {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub name: Name,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GSubgraph), from(models::GSubgraph))]
pub struct GSubgraph {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::GTensor), from(models::GTensor))]
pub struct GTensor {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum Graph {
    Nil(Unit),
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

impl From<Graph> for models::Graph {
    fn from(value: Graph) -> Self {
        match value {
            Graph::Nil(_) => Self::Nil,
            Graph::Vertex(v) => Self::Vertex(v.into()),
            Graph::Var(v) => Self::Var(v.into()),
            Graph::Nominate(v) => Self::Nominate(v.into()),
            Graph::EdgeAnon(v) => Self::EdgeAnon(v.into()),
            Graph::EdgeNamed(v) => Self::EdgeNamed(v.into()),
            Graph::RuleAnon(v) => Self::RuleAnon(v.into()),
            Graph::RuleNamed(v) => Self::RuleNamed(v.into()),
            Graph::Subgraph(v) => Self::Subgraph(v.into()),
            Graph::Tensor(v) => Self::Tensor(v.into()),
        }
    }
}

impl From<models::Graph> for Graph {
    fn from(value: models::Graph) -> Self {
        match value {
            models::Graph::Nil => Self::Nil(Default::default()),
            models::Graph::Vertex(v) => Self::Vertex(v.into()),
            models::Graph::Var(v) => Self::Var(v.into()),
            models::Graph::Nominate(v) => Self::Nominate(v.into()),
            models::Graph::EdgeAnon(v) => Self::EdgeAnon(v.into()),
            models::Graph::EdgeNamed(v) => Self::EdgeNamed(v.into()),
            models::Graph::RuleAnon(v) => Self::RuleAnon(v.into()),
            models::Graph::RuleNamed(v) => Self::RuleNamed(v.into()),
            models::Graph::Subgraph(v) => Self::Subgraph(v.into()),
            models::Graph::Tensor(v) => Self::Tensor(v.into()),
        }
    }
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
