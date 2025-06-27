use poem_openapi::Object;
use structural_convert::StructuralConvert;

use crate::ai_agents::models;
use crate::common::api::dtos::{PreparedContract, SignedContract};

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
pub struct CreateTestwalletResp {
    pub key: String,
}

impl From<models::CreateTestwalletResp> for CreateTestwalletResp {
    fn from(value: models::CreateTestwalletResp) -> Self {
        Self {
            key: value.key.display_secret().to_string(),
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeployTestReq))]
pub struct DeployTestReq {
    pub env: Option<String>,
    pub test: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployTestResp))]
pub struct DeployTestResp {
    pub env_contract: Option<PreparedContract>,
    pub test_contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedTestReq))]
pub struct DeploySignedTestReq {
    pub env: Option<SignedContract>,
    pub test: SignedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployAgentResp))]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
}
