use poem_openapi::{Enum, Object, Union};
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
    pub filesystem: Option<Directory>,
}

#[derive(Debug, Clone, StructuralConvert, Union)]
#[oai(discriminator_name = "type", rename_all = "lowercase")]
#[convert(into(models::Filesystem), from(models::Filesystem))]
pub enum Filesystem {
    Directory(Directory),
    File(File),
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::Directory), from(models::Directory))]
pub struct Directory {
    pub name: String,
    pub members: Vec<Filesystem>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::File), from(models::File))]
pub struct File {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub filesystem: Option<Directory>,
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
#[convert(into(models::DeployAgentReq))]
pub struct DeployAgentReq {
    pub welcome_message: String,
    pub input_prompt: String,
    pub access: Access,
}

#[derive(Debug, Clone, Eq, PartialEq, StructuralConvert, Enum)]
#[convert(into(models::Access))]
pub enum Access {
    Private,
    Public,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeployAgentResp))]
pub struct DeployAgentResp {}
