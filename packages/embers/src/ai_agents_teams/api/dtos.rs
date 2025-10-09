use chrono::{DateTime, Utc};
use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::ai_agents_teams::models;
use crate::common::api::dtos::{PreparedContract, Stringified};
use crate::common::models::{PositiveNonZero, WalletAddress};

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
    pub created_at: Stringified<DateTime<Utc>>,
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::CreateAgentsTeamReq))]
pub struct CreateAgentsTeamReq {
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Stringified<models::Graph>>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Stringified<models::Graph>>,
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

#[derive(Debug, Clone, Object)]
pub struct DeployAgentsTeam {
    pub id: String,
    pub version: String,
    pub address: Stringified<WalletAddress>,
    pub phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Object)]
pub struct DeployGraph {
    pub graph: Stringified<models::Graph>,
    pub phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum DeployAgentsTeamReq {
    AgentsTeam(DeployAgentsTeam),
    Graph(DeployGraph),
}

impl From<DeployAgentsTeamReq> for models::DeployAgentsTeamReq {
    fn from(value: DeployAgentsTeamReq) -> Self {
        match value {
            DeployAgentsTeamReq::AgentsTeam(deploy) => Self::AgentsTeam {
                id: deploy.id,
                version: deploy.version,
                address: deploy.address.0,
                phlo_limit: deploy.phlo_limit.0,
            },
            DeployAgentsTeamReq::Graph(deploy) => Self::Graph {
                graph: deploy.graph.0,
                phlo_limit: deploy.phlo_limit.0,
            },
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployAgentsTeamResp))]
pub struct DeployAgentsTeamResp {
    pub name: String,
    pub contract: PreparedContract,
}
