use chrono::{DateTime, Utc};

use crate::common::models::{PositiveNonZero, PreparedContract, WalletAddress};

#[derive(Debug, Clone)]
pub struct Agents {
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone)]
pub struct AgentHeader {
    pub id: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateAgentReq {
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub shard: Option<String>,
    pub logo: Option<String>,
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
pub struct DeleteAgentResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub enum DeployAgentReq {
    Agent {
        id: String,
        version: String,
        address: WalletAddress,
        phlo_limit: PositiveNonZero<i64>,
    },
    Code {
        code: String,
        phlo_limit: PositiveNonZero<i64>,
    },
}

#[derive(Debug, Clone)]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
}
