#![allow(dead_code)]

use askama::Template;
use firefly_client::models::SignedCode;
use secp256k1::SecretKey;

use crate::common::models::PreparedContract;

#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents/init.rho", escape = "none")]
pub struct InitAgentsEnv;

#[derive(Debug, Clone, Template)]
#[template(path = "ai_agents/init_testnet.rho", escape = "none")]
pub struct InitAgentsTestnetEnv;

#[derive(Debug, Clone)]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone)]
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
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub name: String,
    pub shard: Option<String>,
    pub code: Option<String>,
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
pub struct CreateTestwalletResp {
    pub key: SecretKey,
}

#[derive(Debug, Clone)]
pub struct DeployTestReq {
    pub env: Option<String>,
    pub test: String,
}

#[derive(Debug, Clone)]
pub struct DeployTestResp {
    pub env_contract: Option<PreparedContract>,
    pub test_contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeploySignedTestReq {
    pub env: Option<SignedCode>,
    pub test: SignedCode,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Debug,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum DeploySignedTestResp {
    EnvDeployFailed { error: String },
    TestDeployFailed { error: String },
    Ok { logs: Vec<Log> },
}

#[derive(Debug, Clone)]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
}
