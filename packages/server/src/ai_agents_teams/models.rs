use askama::Template;

use crate::common::models::PreparedContract;

#[allow(dead_code)]
#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents_teams/init.rho", escape = "none")]
pub struct InitAgentsTeamsEnv;

#[derive(Debug, Clone)]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone)]
pub struct AgentsTeamHeader {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateAgentsTeamReq {
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
    pub graph_ast: Option<Graph>,
}

#[derive(Debug, Clone)]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
    pub graph_ast: Option<Graph>,
}

#[derive(Debug, Clone)]
pub struct CreateAgentsTeamResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentsTeamReq = CreateAgentsTeamReq;

#[derive(Debug, Clone)]
pub struct SaveAgentsTeamResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub enum Name {
    Wildcard,
    VVar(String),
    GVar(String),
    QuoteGraph(Box<Graph>),
    QuoteVertex(Box<Vertex>),
}

#[derive(Debug, Clone)]
pub struct Vertex {
    pub name: Name,
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub graph: Box<Graph>,
    pub var: String,
    pub vertex: Vertex,
}

#[derive(Debug, Clone)]
pub struct GVertex {
    pub graph: Box<Graph>,
    pub vertex: Vertex,
}

#[derive(Debug, Clone)]
pub struct GVar {
    pub graph: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone)]
pub struct GEdgeAnon {
    pub binding_1: Binding,
    pub binding_2: Binding,
}

#[derive(Debug, Clone)]
pub struct GEdgeNamed {
    pub binding_1: Binding,
    pub binding_2: Binding,
    pub name: Name,
}

#[derive(Debug, Clone)]
pub struct GRuleAnon {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone)]
pub struct GRuleNamed {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub name: Name,
}

#[derive(Debug, Clone)]
pub struct GSubgraph {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
    pub var: String,
}

#[derive(Debug, Clone)]
pub struct GTensor {
    pub graph_1: Box<Graph>,
    pub graph_2: Box<Graph>,
}

#[derive(Debug, Clone)]
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
