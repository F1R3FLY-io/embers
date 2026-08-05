use chrono::{DateTime, Utc};
use firefly_client::models::WalletAddress;
use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, SignedContract, Stringified};
use crate::domain::agents::models;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agents))]
pub struct Agents {
    /// The wallet's agents, latest version each.
    pub agents: Vec<AgentHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentHeader))]
pub struct AgentHeader {
    /// Agent identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// Latest version identifier (`UUIDv7`).
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// When this version was last deployed standalone; absent if never.
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    /// Agent name.
    pub name: String,
    /// Agent description.
    pub description: Option<String>,
    /// Declared shard label recorded with the agent.
    pub shard: Option<String>,
    /// Logo reference recorded with the agent.
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateAgentReq {
    /// Agent name.
    pub name: String,
    /// Agent description.
    pub description: Option<String>,
    /// Declared shard label recorded with the agent.
    pub shard: Option<String>,
    /// Logo reference recorded with the agent.
    pub logo: Option<String>,
    /// The agent's Rholang source.
    pub code: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::Agent))]
pub struct Agent {
    /// Agent identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// This version's identifier (`UUIDv7`).
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// When this version was last deployed standalone; absent if never.
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    /// Agent name.
    pub name: String,
    /// Agent description.
    pub description: Option<String>,
    /// Declared shard label recorded with the agent.
    pub shard: Option<String>,
    /// Logo reference recorded with the agent.
    pub logo: Option<String>,
    /// The agent's Rholang source; absent when the version was written
    /// without one.
    pub code: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateAgentResp {
    /// The minted agent identity (`UUIDv4`).
    pub id: String,
    /// The first version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned creation contract bytes to sign.
    pub contract: PreparedContract,
}

pub type SaveAgentReq = CreateAgentReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveAgentResp {
    /// The appended version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned save contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteAgentResp {
    /// The unsigned deletion contract bytes to sign.
    pub contract: PreparedContract,
}

/// Deploy a stored agent version, read from the owning wallet's record.
#[derive(Debug, Clone, Hash, Object)]
pub struct DeployAgent {
    /// Stored agent identity (`UUIDv4`).
    id: String,
    /// Exact version to deploy (`UUIDv7`).
    version: String,
    /// The owning wallet address, where the record is read from.
    address: Stringified<WalletAddress>,
}

/// Deploy raw Rholang source verbatim.
#[derive(Debug, Clone, Hash, Object)]
pub struct DeployCode {
    /// Raw Rholang source to deploy verbatim.
    code: String,
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
            },
            DeployAgentReq::Code(deploy) => Self::Code { code: deploy.code },
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeployResp))]
pub struct DeployAgentResp {
    /// The unsigned main contract bytes to sign.
    pub contract: PreparedContract,
    /// The unsigned system contract bytes; present only when the deployment
    /// needs one.
    pub system: Option<PreparedContract>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedReq))]
pub struct DeploySignedAgentReq {
    /// The signed main contract.
    pub contract: SignedContract,
    /// The signed system contract; present exactly when prepare returned one.
    pub system: Option<SignedContract>,
}
