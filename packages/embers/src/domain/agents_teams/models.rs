use std::convert::Infallible;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use firefly_client::models::{SignedCode, Uri, WalletAddress};

use crate::domain::agents_teams::runtime::ResumeCommand;
pub use crate::domain::agents_teams::workflow::WorkflowDefinition;
use crate::domain::common::{PreparedContract, RegistryDeploy};

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

/// Mutex serializing all access to the `graphl_parser` C FFI.
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
    pub definition: Option<WorkflowDefinition>,
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
    pub definition: Option<WorkflowDefinition>,
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
        deploy: RegistryDeploy,
    },
    Graph {
        graph: Graph,
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
    pub agents_team: Uri,
}

#[derive(Debug, Clone)]
pub struct RunResp {
    pub run_id: String,
    pub contract: PreparedContract,
    pub created_at: DateTime<Utc>,
    pub durable: bool,
    pub definition_hash: Option<String>,
    pub request_content_hash: String,
    pub request_capability_hash: String,
    pub continuation_content_hash: Option<String>,
    pub continuation_capability_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRunReq {
    pub contract: SignedCode,
    pub prepare_request: RunReq,
    pub prepare_response: RunResp,
}

#[derive(Debug, Clone)]
pub struct RunCommandReq {
    pub run_id: String,
    pub expected_revision: u64,
    pub command: ResumeCommand,
}

#[derive(Debug, Clone)]
pub struct RunCommandResp {
    pub command_id: String,
    pub command_class: String,
    pub target_wait_id: Option<String>,
    pub contract: PreparedContract,
    pub issued_at: DateTime<Utc>,
    pub command_content_hash: String,
    pub command_capability_hash: String,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRunCommandReq {
    pub contract: SignedCode,
    pub prepare_request: RunCommandReq,
    pub prepare_response: RunCommandResp,
}

#[derive(Debug, Clone)]
pub struct RevealOutputReq {
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct RevealOutputResp {
    pub reveal_id: String,
    pub contract: PreparedContract,
    pub issued_at: DateTime<Utc>,
    pub output_content_hash: String,
    pub output_capability_hash: String,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRevealOutputReq {
    pub contract: SignedCode,
    pub prepare_request: RevealOutputReq,
    pub prepare_response: RevealOutputResp,
}

#[derive(Debug, Clone)]
pub struct RunState {
    pub run_id: String,
    pub agents_team: Uri,
    /// The grant a channel service initiated this run under, when there was
    /// one. Present on capability-gated single-run reads; the public run
    /// enumeration scrubs it, because grant records are public and name their
    /// grantor, and carrying the link there would de-anonymize the owner the
    /// ledger deliberately scrubs.
    pub grant_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub events: Vec<RunEvent>,
    pub definition_hash: Option<String>,
    pub revision: u64,
    pub request_content_hash: Option<String>,
    pub request_capability_hash: Option<String>,
    pub continuation_content_hash: Option<String>,
    pub continuation_capability_hash: Option<String>,
    pub command_id: Option<String>,
    pub command_content_hash: Option<String>,
    pub command_capability_hash: Option<String>,
    pub command_actor: Option<String>,
    pub command_class: Option<String>,
    pub command_wait_id: Option<String>,
    pub approval_authority_grants: Vec<String>,
    pub wait_ids: Vec<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub output_content_hash: Option<String>,
    pub output_capability_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub sequence: u64,
    pub event_type: String,
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

/// A standing, revocable, on-chain delegation: "channel identity `grantee` may
/// initiate runs of `agents_team` owned by me, within `caps`, and may exercise
/// exactly the `authority` I delegated on those runs."
///
/// The grantor is derived on chain from the creating deploy's signer and is
/// never accepted as input. Grants are public authorization facts; the only
/// secret involved is the optional subject bearer token, of which the chain
/// stores only a blake2b-256 hash.
#[derive(Debug, Clone)]
pub struct Grant {
    pub grant_id: String,
    pub grantor: String,
    pub grantee: String,
    pub channel: String,
    pub channel_principal: String,
    pub agents_team: Uri,
    pub caps: GrantCaps,
    pub authority: GrantAuthority,
    pub require_token_hash: Option<String>,
    pub min_auth: String,
    pub created_at: DateTime<Utc>,
    pub revoked: bool,
    pub runs_today: GrantRunsToday,
}

#[derive(Debug, Clone)]
pub struct GrantCaps {
    /// UTC-epoch-day run ceiling; `None` means uncapped. The window resets by
    /// day-number comparison, so up to twice the cap can land across one
    /// midnight boundary; the cap is an abuse throttle, not an invariant.
    pub max_runs_per_day: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GrantAuthority {
    /// Whether the grantee may reveal outputs of runs initiated under this
    /// grant, for delivery over the channel.
    pub reveal: bool,
    /// The command classes (subset of the run-command envelope kinds) the
    /// grantee may issue on runs initiated under this grant.
    pub commands: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GrantRunsToday {
    pub day: i64,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub struct CreateGrantReq {
    pub grantee: String,
    pub channel: String,
    pub channel_principal: String,
    pub agents_team: Uri,
    pub caps: GrantCaps,
    pub authority: GrantAuthority,
    /// When true the server mints a random subject bearer token, returns its
    /// plaintext exactly once in the prepare response, and the contract stores
    /// only the token's blake2b-256 hex digest.
    pub require_token: bool,
    pub min_auth: String,
}

#[derive(Debug, Clone)]
pub struct CreateGrantResp {
    pub grant_id: String,
    pub contract: PreparedContract,
    pub issued_at: DateTime<Utc>,
    /// The minted subject token, present exactly once when requested; it is
    /// never stored and cannot be recovered later.
    pub require_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploySignedCreateGrantReq {
    pub contract: SignedCode,
    pub prepare_request: CreateGrantReq,
    pub prepare_response: CreateGrantResp,
}

#[derive(Debug, Clone)]
pub struct RevokeGrantReq {
    pub grant_id: String,
}

#[derive(Debug, Clone)]
pub struct RevokeGrantResp {
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRevokeGrantReq {
    pub contract: SignedCode,
    pub prepare_request: RevokeGrantReq,
    pub prepare_response: RevokeGrantResp,
}

/// Channel-supplied provenance for a run initiated under a grant. Stored only
/// inside the encrypted durable run-request artifact: the principal and
/// subject are PII-adjacent, and the public run ledger is metadata-only by
/// design. The public ledger records only the grant id.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelContext {
    pub channel: String,
    /// Must equal the grant's `channel_principal`; embers fails fast on
    /// mismatch and the channel service enforces the actual routing.
    pub principal: String,
    /// The channel's content-derived idempotency handle for the triggering
    /// message. Recorded for audit; embers does not dedupe on it, channels own
    /// their dedup discipline.
    pub message_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunUnderGrantReq {
    pub grant_id: String,
    pub prompt: String,
    pub channel_context: ChannelContext,
}

#[derive(Debug, Clone)]
pub struct RunUnderGrantResp {
    pub run: RunResp,
    pub grant_id: String,
    /// The team the grant names; the request never carries a team, so the
    /// caller learns the resolved target here.
    pub agents_team: Uri,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRunUnderGrantReq {
    pub contract: SignedCode,
    pub prepare_request: RunUnderGrantReq,
    pub prepare_response: RunUnderGrantResp,
}

/// A team's public reachability record for one channel: the address the
/// channel recognizes, bound on chain by the team's owner. Carries no key
/// material; channel-side secrets (such as mailbox content keys) are minted
/// and custodied by the channel service, never by embers.
#[derive(Debug, Clone)]
pub struct ChannelBinding {
    pub channel: String,
    pub address: String,
    pub agents_team: Uri,
    pub team_id: String,
    pub owner: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PublishToMailReq {
    pub team_id: String,
    pub local_part: String,
    pub mail_domain: String,
}

#[derive(Debug, Clone)]
pub struct PublishToMailResp {
    pub address: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeploySignedPublishToMailReq {
    pub contract: SignedCode,
    pub prepare_request: PublishToMailReq,
    pub prepare_response: PublishToMailResp,
}

#[derive(Debug, Clone)]
pub struct UnpublishMailReq {
    pub team_id: String,
}

#[derive(Debug, Clone)]
pub struct UnpublishMailResp {
    pub address: String,
    pub contract: PreparedContract,
}

#[derive(Debug, Clone)]
pub struct DeploySignedUnpublishMailReq {
    pub contract: SignedCode,
    pub prepare_request: UnpublishMailReq,
    pub prepare_response: UnpublishMailResp,
}

/// Where a team is reachable, per channel, with honestly three-valued
/// reachability: `Some(true)` verified reachable, `Some(false)` verified
/// unreachable, `None` could not be determined and must never be rendered as
/// missing.
#[derive(Debug, Clone)]
pub struct PublishedChannels {
    pub firesky: Option<PublishedState>,
    pub mail: Vec<MailChannelState>,
    pub chat: ChatChannelState,
}

#[derive(Debug, Clone)]
pub struct MailChannelState {
    pub address: String,
    pub active: bool,
    /// Always `None` today: embers deliberately does not probe SMTP, so mail
    /// reachability is unknown by design rather than falsely asserted.
    pub reachable: Option<bool>,
}

/// Chat reachability is derived from grant presence: a team with active chat
/// grants is connectable by exactly those principals. The gateway exposes no
/// queryable status surface, so this is the honest maximum embers can say.
#[derive(Debug, Clone)]
pub struct ChatChannelState {
    pub active_grant_count: u64,
    pub principals: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeploySignedRunOnFireskyReq {
    pub agents_team: Uri,
    pub run_id: String,
    pub reply_to: Option<FireskyReply>,
    pub run: DeploySignedRunReq,
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
