use askama::Template;

use crate::common::models::{PositiveNonZero, PreparedContract, WalletAddress};

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
pub struct Graph(graphl_parser::ast::Graph);

impl Graph {
    pub fn new(graphl: String) -> Result<Self, graphl_parser::ast::Error> {
        graphl_parser::parse_to_ast(graphl).map(Self)
    }

    pub fn to_graphl(self) -> String {
        graphl_parser::ast_to_graphl(self.0).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct CreateAgentsTeamReq {
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<Graph>,
}

#[derive(Debug, Clone)]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<Graph>,
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
pub enum DeployAgentsTeamReq {
    AgentsTeam {
        id: String,
        version: String,
        address: WalletAddress,
        phlo_limit: PositiveNonZero<i64>,
    },
    Graph {
        graph: Graph,
        phlo_limit: PositiveNonZero<i64>,
    },
}

#[derive(Debug, Clone)]
pub struct DeployAgentsTeamResp {
    pub contract: PreparedContract,
}
