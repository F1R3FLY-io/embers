mod create_agent;

use poem_openapi::{Enum, Object};

pub use self::create_agent::*;

#[derive(Debug, Clone, Object)]
pub struct Agents {
    agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, Object)]
pub struct AgentHeader {
    id: String,
    version: String,
    name: String,
    description: Option<String>,
    shard: Option<String>,
}

#[derive(Debug, Clone, Object)]
pub struct Agent {
    id: String,
    version: String,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    filesystem: Option<Directory>,
}

#[derive(Debug, Clone, Object)]
pub struct CreateAgentResp {
    id: String,
    version: String,
}

pub type SaveAgentReq = CreateAgentReq;

#[derive(Debug, Clone, Object)]
pub struct SaveAgentResp {
    version: String,
}

#[derive(Debug, Clone, Object)]
pub struct TestAgentReq {
    code: String,
}

#[derive(Debug, Clone, Object)]
pub struct TestAgentResp {
    logs: Vec<String>,
    result: serde_json::Value,
}

#[derive(Debug, Clone, Object)]
pub struct DeployAgentReq {
    welcome_message: String,
    input_prompt: String,
    access: Access,
}

#[derive(Debug, Clone, Eq, PartialEq, Enum)]
pub enum Access {
    Private,
    Public,
}

#[derive(Debug, Clone, Object)]
pub struct DeployAgentResp {}
