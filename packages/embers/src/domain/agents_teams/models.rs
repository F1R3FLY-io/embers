use std::convert::Infallible;

use chrono::{DateTime, Utc};
use firefly_client::models::{SignedCode, Uri, WalletAddress};

use crate::domain::common::{PositiveNonZero, PreparedContract, RegistryDeploy};

#[derive(Debug, Clone)]
pub struct AgentsTeams {
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone)]
pub struct AgentsTeamHeader {
    pub id: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub last_deploy: Option<DateTime<Utc>>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Hash, Clone)]
pub struct Graph(graphl_parser::ast::Graph);

impl Graph {
    pub fn new(graphl: String) -> Result<Self, graphl_parser::ast::Error> {
        graphl_parser::parse_to_ast(graphl).map(Self)
    }

    pub fn graphl(self) -> String {
        graphl_parser::ast_to_graphl(self.0).unwrap()
    }

    pub fn visit<'a, V, C>(&'a self, state: C, visitor: V) -> C
    where
        V: graphl_parser::Visitor<'a, C, Infallible>,
    {
        graphl_parser::Walker::new(&self.0).visit(state, visitor)
    }

    pub fn try_visit<'a, V, C, E>(&'a self, state: C, visitor: V) -> Result<C, E>
    where
        V: graphl_parser::Visitor<'a, C, E>,
    {
        graphl_parser::Walker::new(&self.0).try_visit(state, visitor)
    }
}

#[derive(Debug, Clone)]
pub struct CreateReq {
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Graph>,
}

#[derive(Debug, Clone)]
pub struct AgentsTeam {
    pub id: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub last_deploy: Option<DateTime<Utc>>,
    pub uri: Option<Uri>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
    pub graph: Option<Graph>,
}

#[derive(Debug, Clone)]
pub struct CreateResp {
    pub id: String,
    pub version: String,
    pub contract: PreparedContract,
}

pub type SaveReq = CreateReq;

#[derive(Debug, Clone)]
pub struct SaveResp {
    pub version: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeleteResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub enum DeployReq {
    AgentsTeam {
        id: String,
        version: String,
        address: WalletAddress,
        phlo_limit: PositiveNonZero<i64>,
        deploy: RegistryDeploy,
    },
    Graph {
        graph: Graph,
        phlo_limit: PositiveNonZero<i64>,
        deploy: RegistryDeploy,
    },
}

#[derive(Debug, Clone)]
pub struct DeployResp {
    pub contract: PreparedContract,
    pub system: Option<PreparedContract>,
}

#[derive(Debug, Clone)]
pub struct DeploySignedReq {
    pub contract: SignedCode,
    pub system: Option<SignedCode>,
}

#[derive(Debug, Clone)]
pub struct RunReq {
    pub prompt: String,
    pub phlo_limit: PositiveNonZero<i64>,
    pub agents_team: Uri,
}

#[derive(Debug, Clone)]
pub struct RunResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct PublishToFireskyReq {
    pub pds_url: String,
    pub email: String,
    pub handle: String,
    pub password: String,
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PublishToFireskyResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct FireskyCredentials {
    pub pds_url: String,
    pub email: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRunOnFireskyReq {
    pub contract: SignedCode,
    pub agents_team: Uri,
    pub reply_to: Option<FireskyReply>,
}

#[derive(Debug, Clone)]
pub struct FireskyReply {
    pub parent: PostRef,
    pub root: PostRef,
}

#[derive(Debug, Clone)]
pub struct PostRef {
    pub cid: String,
    pub uri: String,
}

#[derive(Debug, Clone)]
pub struct EncryptedMsg {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}
