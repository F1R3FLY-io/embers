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
}

#[derive(Debug, Clone)]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
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
