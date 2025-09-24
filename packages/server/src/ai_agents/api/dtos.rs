use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::ai_agents::models;
use crate::common::api::dtos::{PreparedContract, Stringified};
use crate::common::models::{PositiveNonZero, WalletAddress};

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agents))]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentHeader))]
pub struct AgentHeader {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::CreateAgentReq))]
pub struct CreateAgentReq {
    pub name: String,
    pub shard: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::CreateAgentResp))]
pub struct CreateAgentResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentReq = CreateAgentReq;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::SaveAgentResp))]
pub struct SaveAgentResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Object)]
pub struct DeployAgent {
    id: String,
    version: String,
    address: Stringified<WalletAddress>,
    phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Object)]
pub struct DeployCode {
    code: String,
    phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum DeployAgentReq {
    Agent(DeployAgent),
    Code(DeployCode),
}

impl From<DeployAgentReq> for models::DeployAgentReq {
    fn from(value: DeployAgentReq) -> Self {
        match value {
            DeployAgentReq::Agent(deploy) => Self::Agent {
                id: deploy.id,
                version: deploy.version,
                address: deploy.address.0,
                phlo_limit: deploy.phlo_limit.0,
            },
            DeployAgentReq::Code(deploy) => Self::Code {
                code: deploy.code,
                phlo_limit: deploy.phlo_limit.0,
            },
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployAgentResp))]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
}
