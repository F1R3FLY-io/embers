use std::convert::Infallible;
use std::sync::Mutex;

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
    pub uri: Option<Uri>,
    pub name: String,
    pub description: Option<String>,
    pub shard: Option<String>,
    pub logo: Option<String>,
}

/// Mutex serializing all access to the graphl_parser C FFI.
///
/// The underlying Flex/Bison parser and C printer use global mutable state
/// (`buf_`, `cur_`, `buf_size`, `_n_`, and the Flex scanner buffer).
/// Concurrent calls from different threads corrupt this state, causing
/// intermittent `InvalidGraphL` / `InvalidCString` parse failures.
static GRAPHL_PARSER_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Hash, Clone)]
pub struct Graph(graphl_parser::ast::Graph);

impl Graph {
    pub fn new(graphl: String) -> Result<Self, graphl_parser::ast::Error> {
        let _guard = GRAPHL_PARSER_LOCK
            .lock()
            .expect("GRAPHL_PARSER_LOCK poisoned");
        graphl_parser::parse_to_ast(graphl).map(Self)
    }

    pub fn graphl(self) -> String {
        let _guard = GRAPHL_PARSER_LOCK
            .lock()
            .expect("GRAPHL_PARSER_LOCK poisoned");
        graphl_parser::ast_to_graphl(self.0).expect("ast_to_graphl failed")
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
    /// See the blockchain-layer counterpart: optional because publishes recorded
    /// before these were captured must remain decryptable.
    pub handle: Option<String>,
    pub did: Option<String>,
}

/// A team whose latest version uses a given agent.
///
/// Deleting that agent would leave the team undeployable with no way to repair it,
/// so deletion names these before proceeding.
#[derive(Debug, Clone)]
pub struct ReferencingTeam {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Whether any deployment of this team is published. A published team keeps
    /// answering mentions from its deployed contract even after its record is
    /// deleted, so deletion has to say so.
    pub published: bool,
    /// The handle it was published under, when the publication recorded one.
    /// `None` with `published` true means an older publication whose handle was
    /// never captured -- still live, just not nameable.
    pub published_handle: Option<String>,
}

/// What is known about a team's publication, as far as the chain records it.
#[derive(Debug, Clone)]
pub struct PublishedState {
    /// The handle the team was published under, when the publish recorded one.
    pub handle: Option<String>,
    pub did: Option<String>,
    /// Whether the account still exists on the PDS it was published to.
    ///
    /// `None` when it could not be determined -- an unreachable PDS, or a publish
    /// predating the handle being recorded. Distinguishing "absent" from "unknown"
    /// matters: the first is a divergence to report, the second is not.
    pub account_exists: Option<bool>,
    /// The deployment that is published, which is not necessarily the newest.
    pub uri: Uri,
    pub published_version: String,
    /// Whether the publication is currently in force -- that is, whether the
    /// account still carries the `com.f1r3sky.agentsteam.config` record that makes
    /// f1r3sky treat the handle as a team. Mentions fire only while it is present.
    ///
    /// Distinct from `account_exists` because unpublishing removes the record and
    /// keeps the account. The chain-side credential cannot be removed at all -- the
    /// env stores tokens in a push-only stack -- so a publication is permanent
    /// history and this is the part that actually varies.
    ///
    /// `None` when it could not be determined, for the same reasons as
    /// `account_exists`.
    pub active: Option<bool>,
    /// Whether the published deployment is still the team's most recent one.
    ///
    /// False means a mention runs an older build than the editor shows, because a
    /// deploy mints a new uri while the published config record keeps naming the
    /// old one. Nothing reported this, which is what makes "I re-deployed but
    /// nothing changed" so hard to diagnose.
    pub is_latest_deployment: bool,
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
