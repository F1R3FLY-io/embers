use poem_openapi::Object;
use structural_convert::StructuralConvert;

use crate::ai_agents::models;
use crate::common::api::dtos::PreparedContract;

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

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::TestAgentReq))]
pub struct TestAgentReq {
    pub code: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::TestAgentResp))]
pub struct TestAgentResp {
    pub logs: Vec<String>,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployAgentResp))]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
}
