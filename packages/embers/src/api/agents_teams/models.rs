use chrono::{DateTime, Utc};
use firefly_client::models::{Uri, WalletAddress};
use poem_openapi::{Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, RegistryDeploy, SignedContract, Stringified};
use crate::domain::agents_teams::models;
use crate::domain::common::PositiveNonZero;

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeams))]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeamHeader))]
pub struct AgentsTeamHeader {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    pub uri: Option<Stringified<Uri>>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateAgentsTeamReq {
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Stringified<models::Graph>>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeam))]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub created_at: Stringified<DateTime<Utc>>,
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    pub uri: Option<Stringified<Uri>>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Stringified<models::Graph>>,
}

/// Whether a team is published to F1R3SKY, and whether the account it was
/// published under is still there.
///
/// `account_exists` is deliberately three-valued. `Some(false)` is a real
/// divergence — the chain records a publication whose account has been destroyed,
/// which is what a wiped development PDS leaves behind. `None` means the question
/// could not be answered, from an unreachable PDS or a publish recorded before
/// handles were captured, and must not be presented as though the account were
/// missing.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::PublishedState))]
pub struct PublishedState {
    pub handle: Option<String>,
    pub did: Option<String>,
    pub account_exists: Option<bool>,
    pub uri: Stringified<Uri>,
    pub published_version: String,
    pub is_latest_deployment: bool,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateAgentsTeamResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveAgentsTeamReq = CreateAgentsTeamReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveAgentsTeamResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteAgentsTeamResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct DeployAgentsTeam {
    pub id: String,
    pub version: String,
    pub address: Stringified<WalletAddress>,
    pub phlo_limit: Stringified<PositiveNonZero<i64>>,
    pub deploy: RegistryDeploy,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct DeployGraph {
    pub graph: Stringified<models::Graph>,
    pub phlo_limit: Stringified<PositiveNonZero<i64>>,
    pub deploy: RegistryDeploy,
}

#[derive(Debug, Clone, Hash, Union)]
#[oai(one_of = true, discriminator_name = "type")]
pub enum DeployAgentsTeamReq {
    AgentsTeam(DeployAgentsTeam),
    Graph(DeployGraph),
}

impl From<DeployAgentsTeamReq> for models::DeployReq {
    fn from(value: DeployAgentsTeamReq) -> Self {
        match value {
            DeployAgentsTeamReq::AgentsTeam(deploy) => Self::AgentsTeam {
                id: deploy.id,
                version: deploy.version,
                address: deploy.address.0,
                phlo_limit: deploy.phlo_limit.0,
                deploy: deploy.deploy.into(),
            },
            DeployAgentsTeamReq::Graph(deploy) => Self::Graph {
                graph: deploy.graph.0,
                phlo_limit: deploy.phlo_limit.0,
                deploy: deploy.deploy.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeployResp))]
pub struct DeployAgentsTeamResp {
    pub contract: PreparedContract,
    pub system: Option<PreparedContract>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedReq))]
pub struct DeploySignedAgentsTeamReq {
    pub contract: SignedContract,
    pub system: Option<SignedContract>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::RunReq))]
pub struct RunReq {
    pub prompt: String,
    pub phlo_limit: Stringified<PositiveNonZero<i64>>,
    pub agents_team: Stringified<Uri>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RunResp))]
pub struct RunResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::PublishToFireskyReq))]
pub struct PublishToFireskyReq {
    pub pds_url: String,
    pub email: String,
    pub handle: String,
    pub password: String,
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::PublishToFireskyResp))]
pub struct PublishToFireskyResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Object)]
pub struct DeploySignedRunOnFireskyReq {
    pub contract: SignedContract,
    pub reply_to: Option<FireskyReply>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::FireskyReply))]
pub struct FireskyReply {
    pub parent: PostRef,
    pub root: PostRef,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::PostRef))]
pub struct PostRef {
    pub cid: String,
    pub uri: String,
}
