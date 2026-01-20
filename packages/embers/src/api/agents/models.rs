use chrono::{DateTime, Utc};
use firefly_client::models::WalletAddress;
use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, SignedContract, Stringified};
use crate::domain::agents::models;
use crate::domain::common::PositiveNonZero;

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
    pub created_at: Stringified<DateTime<Utc>>,
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateAgentReq {
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agent))]
pub struct Agent {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateAgentResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentReq = CreateAgentReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveAgentResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteAgentResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct DeployAgent {
    id: String,
    version: String,
    address: Stringified<WalletAddress>,
    phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct DeployCode {
    code: String,
    phlo_limit: Stringified<PositiveNonZero<i64>>,
}

#[derive(Debug, Clone, Hash, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum DeployAgentReq {
    Agent(DeployAgent),
    Code(DeployCode),
}

impl From<DeployAgentReq> for models::DeployReq {
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

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeployResp))]
pub struct DeployAgentResp {
    pub contract: PreparedContract,
    pub system: Option<PreparedContract>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedReq))]
pub struct DeploySignedAgentReq {
    pub contract: SignedContract,
    pub system: Option<SignedContract>,
}
