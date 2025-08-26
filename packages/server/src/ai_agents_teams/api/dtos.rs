use poem_openapi::Object;
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
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub graph: Option<String>,
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
