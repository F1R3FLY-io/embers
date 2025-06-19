#![allow(dead_code)]

use askama::Template;
use serde::{Deserialize, Serialize};

use crate::common::models::PreparedContract;

#[derive(Template)]
#[template(path = "ai_agents/init_agents_env.rho", escape = "none")]
pub struct InitAgentsEnv;

#[derive(Debug, Clone, Deserialize)]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentHeader {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateAgentReq {
    pub name: String,
    pub shard: Option<String>,
    pub filesystem: Option<Directory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Filesystem {
    Directory(Directory),
    File(File),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    pub name: String,
    pub members: Vec<Filesystem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub filesystem: Option<Directory>,
}

#[derive(Debug, Clone)]
pub struct CreateAgentResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentReq = CreateAgentReq;

#[derive(Debug, Clone)]
pub struct SaveAgentResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct TestAgentReq {
    pub code: String,
}

#[derive(Debug, Clone)]
pub struct TestAgentResp {
    pub logs: Vec<String>,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DeployAgentReq {
    pub welcome_message: String,
    pub input_prompt: String,
    pub access: Access,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Access {
    Private,
    Public,
}

#[derive(Debug, Clone)]
pub struct DeployAgentResp {}
