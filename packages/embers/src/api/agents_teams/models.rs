use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};
use firefly_client::models::{Uri, WalletAddress};
use poem_openapi::payload::Json;
use poem_openapi::registry::{MetaSchema, MetaSchemaRef};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use poem_openapi::{ApiResponse, Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{
    InternalError,
    PrepareResponse,
    PreparedContract,
    RegistryDeploy,
    SignedContract,
    Stringified,
};
use crate::domain::agents_teams::{
    GrantError,
    PublishToMailError,
    RevealOutputError,
    RunUnderGrantError,
    models,
};

/// JSON transport for the versioned workflow language.  The domain constructor
/// performs full structural and semantic validation before this value can enter
/// a signed prepare request.
#[derive(Debug, Clone, Hash)]
pub struct WorkflowDefinition(pub models::WorkflowDefinition);

impl Type for WorkflowDefinition {
    const IS_REQUIRED: bool = true;
    type RawValueType = Self;
    type RawElementValueType = Self;
    fn name() -> Cow<'static, str> {
        "WorkflowDefinitionV2".into()
    }
    fn schema_ref() -> MetaSchemaRef {
        MetaSchemaRef::Inline(Box::new(MetaSchema::ANY))
    }
    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }
    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(std::iter::once(self))
    }
}

impl ParseFromJSON for WorkflowDefinition {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        models::WorkflowDefinition::new(value.unwrap_or_default())
            .map(Self)
            .map_err(ParseError::custom)
    }
}

impl ToJSON for WorkflowDefinition {
    fn to_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(self.0.json_ref()).ok()
    }
}

impl From<WorkflowDefinition> for models::WorkflowDefinition {
    fn from(value: WorkflowDefinition) -> Self {
        value.0
    }
}
impl From<models::WorkflowDefinition> for WorkflowDefinition {
    fn from(value: models::WorkflowDefinition) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonValue(pub serde_json::Value);

impl Eq for JsonValue {}

impl Hash for JsonValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        serde_json::to_string(&self.0)
            .expect("serde_json::Value serialization cannot fail")
            .hash(state);
    }
}

impl Type for JsonValue {
    const IS_REQUIRED: bool = true;
    type RawValueType = Self;
    type RawElementValueType = Self;
    fn name() -> Cow<'static, str> {
        "JsonValue".into()
    }
    fn schema_ref() -> MetaSchemaRef {
        MetaSchemaRef::Inline(Box::new(MetaSchema::ANY))
    }
    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }
    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(std::iter::once(self))
    }
}

impl ParseFromJSON for JsonValue {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        Ok(Self(value.unwrap_or(serde_json::Value::Null)))
    }
}

impl ToJSON for JsonValue {
    fn to_json(&self) -> Option<serde_json::Value> {
        Some(self.0.clone())
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeams))]
pub struct AgentsTeams {
    /// The wallet's teams, latest version each.
    pub agents_teams: Vec<AgentsTeamHeader>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeamHeader))]
pub struct AgentsTeamHeader {
    /// Team identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// Latest version identifier (`UUIDv7`).
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// When this version was last deployed; absent if never.
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    /// Registry URI of the last deployment; the value run invocations
    /// consume.
    pub uri: Option<Stringified<Uri>>,
    /// Team name.
    pub name: String,
    /// Team description.
    pub description: Option<String>,
    /// Declared shard label.
    pub shard: Option<String>,
    /// Logo reference.
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::CreateReq))]
pub struct CreateAgentsTeamReq {
    /// Team name.
    pub name: String,
    /// Team description.
    pub description: Option<String>,
    /// Declared shard label.
    pub shard: Option<String>,
    /// Logo reference.
    pub logo: Option<String>,
    /// The graphl serialization of the canvas.
    pub graph: Option<Stringified<models::Graph>>,
    /// The Workflow v2 definition document; validated permissively at save
    /// time, strictly at deploy.
    pub definition: Option<WorkflowDefinition>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::AgentsTeam))]
pub struct AgentsTeam {
    /// Team identity (`UUIDv4`), stable across versions.
    pub id: String,
    /// This version's identifier (`UUIDv7`).
    pub version: String,
    /// When this version was prepared.
    pub created_at: Stringified<DateTime<Utc>>,
    /// When this version was last deployed; absent if never.
    pub last_deploy: Option<Stringified<DateTime<Utc>>>,
    /// Registry URI of the last deployment; the value run invocations
    /// consume.
    pub uri: Option<Stringified<Uri>>,
    /// Team name.
    pub name: String,
    /// Team description.
    pub description: Option<String>,
    /// Declared shard label.
    pub shard: Option<String>,
    /// Logo reference.
    pub logo: Option<String>,
    /// The graphl serialization of the canvas.
    pub graph: Option<Stringified<models::Graph>>,
    /// The Workflow v2 definition document.
    pub definition: Option<WorkflowDefinition>,
}

/// A team whose latest version uses a given agent, and would break if it were deleted.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::ReferencingTeam))]
pub struct ReferencingTeam {
    /// The referencing team's identity (`UUIDv4`).
    pub id: String,
    /// Its name.
    pub name: String,
    /// The latest version (`UUIDv7`) — the one that references the agent.
    pub version: String,
    /// Whether the team is currently published.
    pub published: bool,
    /// The handle that would stop working meaningfully.
    pub published_handle: Option<String>,
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
    /// The account's handle, when recorded.
    pub handle: Option<String>,
    /// The account's DID, when recorded.
    pub did: Option<String>,
    /// Three-valued PDS reconciliation: true when the account answers, false
    /// when the chain records a publication whose account is gone, null when
    /// the question could not be answered.
    pub account_exists: Option<bool>,
    /// Whether the config record is present — the team answers mentions.
    pub active: Option<bool>,
    /// The published deployment's registry URI.
    pub uri: Stringified<Uri>,
    /// The team version (`UUIDv7`) that deployment was built from.
    pub published_version: String,
    /// Whether the published deployment is still the team's newest.
    pub is_latest_deployment: bool,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateResp))]
pub struct CreateAgentsTeamResp {
    /// The minted team identity (`UUIDv4`).
    pub id: String,
    /// The first version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned creation contract bytes to sign.
    pub contract: PreparedContract,
}

pub type SaveAgentsTeamReq = CreateAgentsTeamReq;

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::SaveResp))]
pub struct SaveAgentsTeamResp {
    /// The appended version's identifier (`UUIDv7`).
    pub version: String,
    /// The unsigned save contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeleteResp))]
pub struct DeleteAgentsTeamResp {
    /// The unsigned deletion contract bytes to sign.
    pub contract: PreparedContract,
}

/// Deploy a stored team version, read from the owning wallet's record.
#[derive(Debug, Clone, Hash, Object)]
pub struct DeployAgentsTeam {
    /// Stored team identity (`UUIDv4`).
    pub id: String,
    /// Exact version to deploy (`UUIDv7`).
    pub version: String,
    /// Owning wallet address; record and agent reads resolve against it.
    pub address: Stringified<WalletAddress>,
    /// The registry claim authorizing publication under the target URI.
    pub deploy: RegistryDeploy,
}

/// Compile and deploy a raw graphl term.
#[derive(Debug, Clone, Hash, Object)]
pub struct DeployGraph {
    /// Raw graphl term to compile.
    pub graph: Stringified<models::Graph>,
    /// The registry claim authorizing publication under the target URI.
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
                deploy: deploy.deploy.into(),
            },
            DeployAgentsTeamReq::Graph(deploy) => Self::Graph {
                graph: deploy.graph.0,
                deploy: deploy.deploy.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::DeployResp))]
pub struct DeployAgentsTeamResp {
    /// The unsigned compiled team contract bytes to sign.
    pub contract: PreparedContract,
    /// The unsigned record-update contract bytes; present only when the
    /// deployment needs one.
    pub system: Option<PreparedContract>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::DeploySignedReq))]
pub struct DeploySignedAgentsTeamReq {
    /// The signed compiled team contract.
    pub contract: SignedContract,
    /// The signed record-update contract; present exactly when prepare
    /// returned one.
    pub system: Option<SignedContract>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::RunReq))]
pub struct RunReq {
    /// The invocation input.
    pub prompt: String,
    /// The deployed team's registry URI.
    pub agents_team: Stringified<Uri>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RunResp))]
pub struct RunResp {
    /// The run capability (`UUIDv7`), minted at prepare.
    pub run_id: String,
    /// The unsigned invocation contract bytes to sign.
    pub contract: PreparedContract,
    /// Prepare instant.
    pub created_at: Stringified<DateTime<Utc>>,
    /// Whether the run uses the durable ledger.
    pub durable: bool,
    /// Hash pinning the executed definition.
    pub definition_hash: Option<String>,
    /// Content commitment over the invocation artifact.
    pub request_content_hash: String,
    /// Capability commitment over the invocation artifact.
    pub request_capability_hash: String,
    /// Content commitment over the durable continuation.
    pub continuation_content_hash: Option<String>,
    /// Capability commitment over the durable continuation.
    pub continuation_capability_hash: Option<String>,
}

impl From<RunResp> for models::RunResp {
    fn from(value: RunResp) -> Self {
        Self {
            run_id: value.run_id,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
            created_at: value.created_at.0,
            durable: value.durable,
            definition_hash: value.definition_hash,
            request_content_hash: value.request_content_hash,
            request_capability_hash: value.request_capability_hash,
            continuation_content_hash: value.continuation_content_hash,
            continuation_capability_hash: value.continuation_capability_hash,
        }
    }
}

/// The authenticated run-command kinds the ledger adjudicates.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Enum)]
#[oai(rename_all = "camelCase")]
pub enum RunCommandKind {
    Approve,
    Cancel,
    Event,
    Migrate,
    Reject,
    Retry,
    Signal,
    TimerFired,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct RunCommandReq {
    /// The revision the command fences on; read it from the run state. A
    /// stale value is not an HTTP error: the command deploys but does not
    /// apply.
    pub expected_revision: Stringified<u64>,
    /// The command kind being issued.
    #[oai(rename = "type")]
    pub command_type: RunCommandKind,
    /// The exact qualified wait addressed; required for signals, events,
    /// decisions, timer fires, and retries.
    pub wait_id: Option<String>,
    /// Event or signal name; required where the kind carries one.
    pub name: Option<String>,
    /// Signal, event, or decision payload.
    pub payload: Option<JsonValue>,
    /// Per-actor decision identity for approvals; blocks decision replay.
    pub decision_id: Option<String>,
    /// Required for `timerFired`; must equal the wait's recorded deadline
    /// exactly, or the kernel rejects the fire in the readback.
    pub expected_deadline: Option<Stringified<DateTime<Utc>>>,
    /// Reject or cancel reason.
    pub reason: Option<String>,
    /// Required for `migrate`: the hash-checked current definition.
    pub from_definition_hash: Option<String>,
    /// Required for `migrate`: the hash-checked target definition.
    pub to_definition_hash: Option<String>,
    /// Required for `migrate`: the target deployment's registry URI.
    pub target_team_uri: Option<String>,
    /// Required for `migrate`: `set`/`remove` patch applied to portable
    /// state.
    pub state_patch: Option<JsonValue>,
}

impl RunCommandReq {
    pub fn into_domain(self, run_id: String) -> anyhow::Result<models::RunCommandReq> {
        use crate::domain::agents_teams::runtime::{ResumeCommand, StatePatchOperation};

        let required = |value: Option<String>, field: &str| {
            value.ok_or_else(|| anyhow::anyhow!("{field} is required for this command"))
        };
        let payload = self
            .payload
            .map_or(serde_json::Value::Null, |value| value.0);
        let command = match self.command_type {
            RunCommandKind::Signal => ResumeCommand::Signal {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
                name: required(self.name, "name")?,
                payload,
            },
            RunCommandKind::Event => ResumeCommand::Event {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
                name: required(self.name, "name")?,
                payload,
            },
            RunCommandKind::TimerFired => ResumeCommand::TimerFired {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
                expected_deadline: self
                    .expected_deadline
                    .map(|deadline| deadline.0)
                    .ok_or_else(|| {
                        anyhow::anyhow!("expectedDeadline is required for this command")
                    })?,
            },
            RunCommandKind::Approve => ResumeCommand::Approve {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
                decision_id: required(self.decision_id, "decisionId")?,
                payload,
            },
            RunCommandKind::Reject => ResumeCommand::Reject {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
                payload,
            },
            RunCommandKind::Cancel => ResumeCommand::Cancel {
                run_id: run_id.clone(),
                reason: required(self.reason, "reason")?,
            },
            RunCommandKind::Retry => ResumeCommand::Retry {
                run_id: run_id.clone(),
                wait_id: required(self.wait_id, "waitId")?,
            },
            RunCommandKind::Migrate => ResumeCommand::Migrate {
                run_id: run_id.clone(),
                from_definition_hash: required(self.from_definition_hash, "fromDefinitionHash")?,
                to_definition_hash: required(self.to_definition_hash, "toDefinitionHash")?,
                target_team_uri: required(self.target_team_uri, "targetTeamUri")?,
                state_patch: serde_json::from_value::<Vec<StatePatchOperation>>(
                    self.state_patch
                        .map_or_else(|| serde_json::json!([]), |value| value.0),
                )?,
            },
        };
        Ok(models::RunCommandReq {
            run_id,
            expected_revision: self.expected_revision.0,
            command,
        })
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RunCommandResp))]
pub struct RunCommandResp {
    /// The minted command identity.
    pub command_id: String,
    /// The authorization class the ledger adjudicates.
    pub command_class: String,
    /// The wait the command addresses.
    pub target_wait_id: Option<String>,
    /// The unsigned claim contract bytes to sign.
    pub contract: PreparedContract,
    /// Prepare instant.
    pub issued_at: Stringified<DateTime<Utc>>,
    /// Content commitment over the encrypted command artifact.
    pub command_content_hash: String,
    /// Capability commitment over the encrypted command artifact.
    pub command_capability_hash: String,
}

impl From<RunCommandResp> for models::RunCommandResp {
    fn from(value: RunCommandResp) -> Self {
        Self {
            command_id: value.command_id,
            command_class: value.command_class,
            target_wait_id: value.target_wait_id,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
            issued_at: value.issued_at.0,
            command_content_hash: value.command_content_hash,
            command_capability_hash: value.command_capability_hash,
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::RunState))]
pub struct RunState {
    /// The run capability (`UUIDv7`).
    pub run_id: String,
    /// The invoked team's registry URI.
    pub agents_team: Stringified<Uri>,
    /// The grant a channel service initiated this run under, when there was
    /// one. Present on this capability-gated read; the public run enumeration
    /// scrubs it, because grant records are public and name their grantor.
    pub grant_id: Option<String>,
    /// Run creation instant.
    pub created_at: Stringified<DateTime<Utc>>,
    /// Ledger status: running, suspended, resuming, completed, failed, or
    /// cancelled.
    pub status: String,
    /// The merged, ordered audit history — event types only.
    pub events: Vec<RunEvent>,
    /// Pin of the executed definition.
    pub definition_hash: Option<String>,
    /// The optimistic-concurrency revision commands fence on.
    pub revision: Stringified<u64>,
    /// Content commitment over the invocation artifact.
    pub request_content_hash: Option<String>,
    /// Capability commitment over the invocation artifact.
    pub request_capability_hash: Option<String>,
    /// Content commitment over the durable continuation.
    pub continuation_content_hash: Option<String>,
    /// Capability commitment over the durable continuation.
    pub continuation_capability_hash: Option<String>,
    /// The last applied command's public claim: its identity.
    pub command_id: Option<String>,
    /// The last applied command's public claim: content commitment.
    pub command_content_hash: Option<String>,
    /// The last applied command's public claim: capability commitment.
    pub command_capability_hash: Option<String>,
    /// The last applied command's public claim: the commanding actor.
    pub command_actor: Option<String>,
    /// The last applied command's public claim: its authorization class.
    pub command_class: Option<String>,
    /// The last applied command's public claim: the wait it addressed.
    pub command_wait_id: Option<String>,
    /// Grant entries (`waitId|address`) naming delegated approvers.
    pub approval_authority_grants: Vec<String>,
    /// Public qualified wait identifiers currently pending.
    pub wait_ids: Vec<String>,
    /// Next scheduled resume instant (parked durable yield or timer).
    pub due_at: Option<Stringified<DateTime<Utc>>>,
    /// Content commitment over the terminal output artifact.
    pub output_content_hash: Option<String>,
    /// Capability commitment over the terminal output artifact.
    pub output_capability_hash: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::RunEvent))]
pub struct RunEvent {
    /// Position in the merged audit order.
    pub sequence: Stringified<u64>,
    /// The event's type label; contents never appear here.
    pub event_type: String,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(into(models::PublishToFireskyReq))]
pub struct PublishToFireskyReq {
    /// `create` or `existing`; omitted preserves legacy create-or-login behavior.
    pub account_mode: Option<String>,
    /// The PDS to host the account.
    pub pds_url: String,
    /// Account email.
    pub email: String,
    /// The handle the team will answer as.
    pub handle: String,
    /// Account password.
    pub password: String,
    /// Invitation code, where the PDS requires invitations.
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::PublishToFireskyResp))]
pub struct PublishToFireskyResp {
    /// The unsigned publication contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Object)]
pub struct DeploySignedRunOnFireskyReq {
    /// The signed invocation.
    pub contract: SignedContract,
    /// Threads the answer under an existing post.
    pub reply_to: Option<FireskyReply>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::FireskyReply))]
pub struct FireskyReply {
    /// The post the reply answers directly.
    pub parent: PostRef,
    /// The thread's root post.
    pub root: PostRef,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(into(models::PostRef))]
pub struct PostRef {
    /// The post record's content identifier.
    pub cid: String,
    /// The post record's AT URI.
    pub uri: String,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RevealOutputResp))]
pub struct RevealOutputResp {
    /// Fresh `UUIDv7` identity minted at prepare; echoed by the on-chain
    /// acknowledgment.
    pub reveal_id: String,
    /// The unsigned `claimOutputReveal` contract bytes to sign.
    pub contract: PreparedContract,
    /// Prepare instant.
    pub issued_at: Stringified<DateTime<Utc>>,
    /// Content commitment over the encrypted output artifact, echoed so the
    /// send can verify nothing moved between prepare and claim.
    pub output_content_hash: String,
    /// Capability commitment over the encrypted output artifact, echoed so
    /// the send can verify nothing moved between prepare and claim.
    pub output_capability_hash: String,
}

impl From<RevealOutputResp> for models::RevealOutputResp {
    fn from(value: RevealOutputResp) -> Self {
        Self {
            reveal_id: value.reveal_id,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
            issued_at: value.issued_at.0,
            output_content_hash: value.output_content_hash,
            output_capability_hash: value.output_capability_hash,
        }
    }
}

/// Typed statuses for the owner-authenticated reveal surface. 403 is honest
/// here rather than a capability leak: run existence is already public
/// through the bearer inspector link, so absence-hiding buys nothing and a
/// 404 would misreport an authorization failure.
#[derive(Debug, ApiResponse)]
pub enum PrepareRevealOutputResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<RevealOutputResp>>),
    #[oai(status = 404)]
    NotFound,
    /// The run exists but has no recorded output artifact yet.
    #[oai(status = 409)]
    NotRevealable,
    #[oai(status = 500)]
    InternalError(Json<InternalError>),
}

impl From<RevealOutputError> for PrepareRevealOutputResponse {
    fn from(error: RevealOutputError) -> Self {
        match error {
            RevealOutputError::NotFound => Self::NotFound,
            RevealOutputError::NotRevealable => Self::NotRevealable,
            // Authorization and envelope consumption belong to the claim
            // deploy, which prepare never issues; either here is an internal
            // inconsistency.
            RevealOutputError::Unauthorized | RevealOutputError::Consumed => Self::InternalError(
                Json(anyhow::anyhow!("unexpected claim-phase failure during prepare").into()),
            ),
            RevealOutputError::Internal(err) => Self::InternalError(Json(err.into())),
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum SendRevealOutputResponse {
    /// The run's final output value, released only over this authenticated
    /// response channel; the public ledger records the occurrence
    /// (`outputRevealed`) but never the value.
    #[oai(status = 200)]
    Ok(Json<serde_json::Value>),
    #[oai(status = 400)]
    BadRequest(Json<InternalError>),
    /// The signer is not the run's on-chain owner.
    #[oai(status = 403)]
    Unauthorized,
    #[oai(status = 404)]
    NotFound,
    /// The run has no recorded output, or the send envelope was already
    /// consumed (a replayed envelope produces no new reveal event); prepare a
    /// fresh reveal.
    #[oai(status = 409)]
    NotRevealable,
    #[oai(status = 500)]
    InternalError(Json<InternalError>),
}

impl From<RevealOutputError> for SendRevealOutputResponse {
    fn from(error: RevealOutputError) -> Self {
        match error {
            RevealOutputError::NotFound => Self::NotFound,
            RevealOutputError::NotRevealable | RevealOutputError::Consumed => Self::NotRevealable,
            RevealOutputError::Unauthorized => Self::Unauthorized,
            RevealOutputError::Internal(err) => Self::InternalError(Json(err.into())),
        }
    }
}

/// A machine-readable failure body for the channel-connector surface: `code`
/// is a stable identifier a channel service branches on, `description` is for
/// humans and logs. Codes are documented per operation in the API reference.
#[derive(Debug, Clone, Object)]
pub struct ErrorBody {
    /// Stable machine-readable failure code.
    pub code: String,
    /// Human-readable failure description.
    pub description: String,
    /// Field names use the API's snake_case request names.
    pub field_errors: Option<std::collections::BTreeMap<String, Vec<String>>>,
}

impl ErrorBody {
    pub fn new(code: &str, description: impl std::fmt::Display) -> Self {
        Self {
            code: code.to_owned(),
            description: description.to_string(),
            field_errors: None,
        }
    }
}

/// The channels a grant may name. Chat platform disambiguation lives inside
/// the opaque principal (already platform- and workspace-scoped), not here.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Enum)]
#[oai(rename_all = "camelCase")]
pub enum GrantChannel {
    Chat,
    Mail,
}

impl GrantChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Mail => "mail",
        }
    }
}

/// Sender-authentication floor a mail grant requires; enforced by the mail
/// bridge against chain-recorded authentication results, never by embers.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Enum)]
#[oai(rename_all = "camelCase")]
pub enum MinAuthLevel {
    None,
    Spf,
    Dkim,
    Dmarc,
}

impl MinAuthLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Spf => "spf",
            Self::Dkim => "dkim",
            Self::Dmarc => "dmarc",
        }
    }
}

/// The command classes a grantor may delegate. `timerFired` belongs to the
/// service scheduler and `migrate` is administrative; neither is delegable.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Enum)]
#[oai(rename_all = "camelCase")]
pub enum GrantCommandClass {
    Approve,
    Cancel,
    Event,
    Reject,
    Retry,
    Signal,
}

impl GrantCommandClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Cancel => "cancel",
            Self::Event => "event",
            Self::Reject => "reject",
            Self::Retry => "retry",
            Self::Signal => "signal",
        }
    }
}

#[derive(Debug, Clone, Hash, Object)]
pub struct GrantCaps {
    /// UTC-epoch-day run ceiling; absent means uncapped. Up to twice the cap
    /// can land across one midnight boundary; the cap is an abuse throttle.
    pub max_runs_per_day: Option<Stringified<i64>>,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct GrantAuthority {
    /// Whether the grantee may reveal outputs of runs initiated under this
    /// grant, for delivery over the channel.
    pub reveal: bool,
    /// The command classes the grantee may issue on runs initiated under this
    /// grant.
    pub commands: Vec<GrantCommandClass>,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct CreateGrantReq {
    /// The channel service's wallet address (the grantee, who signs and pays
    /// run deploys under this grant).
    pub grantee: Stringified<WalletAddress>,
    /// The channel the grant serves.
    pub channel: GrantChannel,
    /// The channel-native principal this grant answers for: the chat
    /// gateway's opaque author hash, or a mail sender address. Embers
    /// string-matches it and never interprets it.
    pub channel_principal: String,
    /// The deployed team's registry URI the grant covers.
    pub agents_team: Stringified<Uri>,
    /// Frequency ceilings.
    pub caps: GrantCaps,
    /// The authority delegated to the grantee.
    pub authority: GrantAuthority,
    /// When true, a random subject bearer token is minted; its plaintext is
    /// returned exactly once in the prepare response and only its blake2b-256
    /// hex digest is stored on chain.
    pub require_token: bool,
    /// Sender-authentication floor for mail-channel grants.
    pub min_auth: MinAuthLevel,
}

impl From<CreateGrantReq> for models::CreateGrantReq {
    fn from(value: CreateGrantReq) -> Self {
        Self {
            grantee: value.grantee.0.as_ref().to_owned(),
            channel: value.channel.as_str().to_owned(),
            channel_principal: value.channel_principal,
            agents_team: value.agents_team.0,
            caps: models::GrantCaps {
                max_runs_per_day: value.caps.max_runs_per_day.map(|cap| cap.0),
            },
            authority: models::GrantAuthority {
                reveal: value.authority.reveal,
                commands: value
                    .authority
                    .commands
                    .into_iter()
                    .map(|class| class.as_str().to_owned())
                    .collect(),
            },
            require_token: value.require_token,
            min_auth: value.min_auth.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::CreateGrantResp))]
pub struct CreateGrantResp {
    /// The minted grant identity (`UUIDv7`).
    pub grant_id: String,
    /// The unsigned grant-creation contract bytes to sign (grantor-signed).
    pub contract: PreparedContract,
    /// Prepare instant.
    pub issued_at: Stringified<DateTime<Utc>>,
    /// The minted subject bearer token, present exactly once when requested;
    /// it is never stored and cannot be recovered later.
    pub require_token: Option<String>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RevokeGrantResp))]
pub struct RevokeGrantResp {
    /// The unsigned revocation contract bytes to sign (grantor-signed).
    pub contract: PreparedContract,
}

impl From<CreateGrantResp> for models::CreateGrantResp {
    fn from(value: CreateGrantResp) -> Self {
        Self {
            grant_id: value.grant_id,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
            issued_at: value.issued_at.0,
            require_token: value.require_token,
        }
    }
}

impl From<RevokeGrantResp> for models::RevokeGrantResp {
    fn from(value: RevokeGrantResp) -> Self {
        Self {
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
        }
    }
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::GrantRunsToday))]
pub struct GrantRunsToday {
    /// The UTC epoch-day the counter covers.
    pub day: Stringified<i64>,
    /// Runs admitted within that day.
    pub count: Stringified<i64>,
}

/// A grant record as the chain holds it: a public authorization fact. The
/// HTTP surface mirrors the observer-readable chain clauses for UI and
/// operations; channel services consume the chain clauses directly.
#[derive(Debug, Clone, Object)]
pub struct Grant {
    /// Grant identity (`UUIDv7`).
    pub grant_id: String,
    /// The granting wallet address; derived on chain from the creating
    /// deploy's signer.
    pub grantor: String,
    /// The channel service's wallet address.
    pub grantee: String,
    /// The channel the grant serves.
    pub channel: String,
    /// The channel-native principal, opaque to embers.
    pub channel_principal: String,
    /// The covered team deployment's registry URI.
    pub agents_team: Stringified<Uri>,
    /// Frequency ceilings.
    pub caps: GrantCaps,
    /// The delegated authority, as recorded.
    pub authority: GrantAuthorityView,
    /// blake2b-256 hex digest of the subject bearer token, when one is
    /// required. The token itself is never stored.
    pub require_token_hash: Option<String>,
    /// Sender-authentication floor for mail-channel grants.
    pub min_auth: String,
    /// Creation instant.
    pub created_at: Stringified<DateTime<Utc>>,
    /// Whether the grant has been revoked. Revocation is delivery-only for
    /// in-flight runs: they complete for their owner, while grantee reveal
    /// and command claims fail from the moment of revocation.
    pub revoked: bool,
    /// The chain-maintained day counter.
    pub runs_today: GrantRunsToday,
}

/// Recorded authority, with classes as strings so future recorded values
/// survive the read path unmodified.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::GrantAuthority))]
pub struct GrantAuthorityView {
    pub reveal: bool,
    pub commands: Vec<String>,
}

impl From<models::Grant> for Grant {
    fn from(value: models::Grant) -> Self {
        Self {
            grant_id: value.grant_id,
            grantor: value.grantor,
            grantee: value.grantee,
            channel: value.channel,
            channel_principal: value.channel_principal,
            agents_team: Stringified(value.agents_team),
            caps: GrantCaps {
                max_runs_per_day: value.caps.max_runs_per_day.map(Stringified),
            },
            authority: value.authority.into(),
            require_token_hash: value.require_token_hash,
            min_auth: value.min_auth,
            created_at: Stringified(value.created_at),
            revoked: value.revoked,
            runs_today: value.runs_today.into(),
        }
    }
}

/// Channel-supplied provenance for a grant-initiated run. Stored only inside
/// the encrypted durable run-request artifact; the public ledger records only
/// the grant id.
#[derive(Debug, Clone, Hash, Object)]
pub struct ChannelContext {
    /// The channel issuing the run; must equal the grant's channel.
    pub channel: GrantChannel,
    /// The channel-native principal; must equal the grant's principal.
    pub principal: String,
    /// The channel's content-derived idempotency handle for the triggering
    /// message. Recorded for audit; channels own their dedup discipline.
    pub message_fingerprint: String,
    /// Mail subject line, when the channel has one.
    pub subject: Option<String>,
}

impl From<ChannelContext> for models::ChannelContext {
    fn from(value: ChannelContext) -> Self {
        Self {
            channel: value.channel.as_str().to_owned(),
            principal: value.principal,
            message_fingerprint: value.message_fingerprint,
            subject: value.subject,
        }
    }
}

#[derive(Debug, Clone, Hash, Object)]
pub struct RunUnderGrantReq {
    /// The grant authorizing this run; the target team comes from it.
    pub grant_id: String,
    /// The invocation input.
    pub prompt: String,
    /// Provenance of the triggering channel message.
    pub channel_context: ChannelContext,
}

impl From<RunUnderGrantReq> for models::RunUnderGrantReq {
    fn from(value: RunUnderGrantReq) -> Self {
        Self {
            grant_id: value.grant_id,
            prompt: value.prompt,
            channel_context: value.channel_context.into(),
        }
    }
}

#[derive(Debug, Clone, Hash, Object)]
pub struct RunUnderGrantResp {
    /// The prepared run, identical in shape to a plain run prepare. The
    /// deploy is signed and paid by the grantee; the run is owned by the
    /// grantor per the on-chain grant.
    pub run: RunResp,
    /// The authorizing grant, echoed.
    pub grant_id: String,
    /// The team the grant names; the request never carries a team, so the
    /// caller learns the resolved target here.
    pub agents_team: Stringified<Uri>,
}

impl From<models::RunUnderGrantResp> for RunUnderGrantResp {
    fn from(value: models::RunUnderGrantResp) -> Self {
        Self {
            run: value.run.into(),
            grant_id: value.grant_id,
            agents_team: Stringified(value.agents_team),
        }
    }
}

impl From<RunUnderGrantResp> for models::RunUnderGrantResp {
    fn from(value: RunUnderGrantResp) -> Self {
        Self {
            run: value.run.into(),
            grant_id: value.grant_id,
            agents_team: value.agents_team.0,
        }
    }
}

#[derive(Debug, Clone, Hash, Object)]
pub struct PublishToMailReq {
    /// The local part of the team's mail address (before the `@`).
    pub local_part: String,
    /// The mail domain the address lives under.
    pub mail_domain: String,
}

impl PublishToMailReq {
    pub fn into_domain(self, team_id: String) -> models::PublishToMailReq {
        models::PublishToMailReq {
            team_id,
            local_part: self.local_part,
            mail_domain: self.mail_domain,
        }
    }
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::PublishToMailResp))]
pub struct PublishToMailResp {
    /// The canonical (lowercased) address the team will be reachable at.
    pub address: String,
    /// The unsigned binding contract bytes to sign (team-owner-signed).
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::UnpublishMailResp))]
pub struct UnpublishMailResp {
    /// The address whose binding is being deactivated.
    pub address: String,
    /// The unsigned deactivation contract bytes to sign (team-owner-signed).
    pub contract: PreparedContract,
}

impl From<PublishToMailResp> for models::PublishToMailResp {
    fn from(value: PublishToMailResp) -> Self {
        Self {
            address: value.address,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
        }
    }
}

impl From<UnpublishMailResp> for models::UnpublishMailResp {
    fn from(value: UnpublishMailResp) -> Self {
        Self {
            address: value.address,
            contract: crate::domain::common::PreparedContract(value.contract.0.0),
        }
    }
}

/// A team's public reachability record for one channel. Carries no key
/// material: channel-side secrets are minted and custodied by the channel
/// service, never by embers.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::ChannelBinding))]
pub struct ChannelBinding {
    /// The channel the binding serves.
    pub channel: String,
    /// The channel-native address (for mail, `local@domain`).
    pub address: String,
    /// The bound team deployment's registry URI.
    pub agents_team: Stringified<Uri>,
    /// The bound team's identity (`UUIDv4`).
    pub team_id: String,
    /// The team owner's wallet address.
    pub owner: String,
    /// Whether the binding is in force; unpublishing flips this off,
    /// reversibly.
    pub active: bool,
    /// Binding creation instant.
    pub created_at: Stringified<DateTime<Utc>>,
}

/// Where a team is reachable, per channel, with honestly three-valued
/// reachability: `true` verified reachable, `false` verified unreachable,
/// absent when the question could not be answered -- which must never be
/// rendered as "missing".
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::PublishedChannels))]
pub struct PublishedChannels {
    /// F1R3SKY publication state, when the team was ever published there.
    pub firesky: Option<PublishedState>,
    /// Mail bindings for the team.
    pub mail: Vec<MailChannelState>,
    /// Chat reachability, derived from grant presence.
    pub chat: ChatChannelState,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::MailChannelState))]
pub struct MailChannelState {
    /// The bound mail address.
    pub address: String,
    /// Whether the binding is in force.
    pub active: bool,
    /// Always absent today: embers deliberately does not probe SMTP, so mail
    /// reachability is unknown by design rather than falsely asserted.
    pub reachable: Option<bool>,
}

/// Chat reachability derived from grant presence: a team with active chat
/// grants is connectable by exactly those principals. The gateway dials out
/// and exposes no status surface, so this is the honest maximum embers can
/// report.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::ChatChannelState))]
pub struct ChatChannelState {
    /// Active (unrevoked) chat grants across all of the team's deployments.
    pub active_grant_count: Stringified<u64>,
    /// The distinct principals those grants name.
    pub principals: Vec<String>,
}

/// Whether an internal failure is the node refusing a deploy for lack of
/// funds; surfaced as its own code so channel services can distinguish "top
/// up the channel wallet" from every other failure. The node reports this in
/// prose, so the match is textual by necessity and documented as such.
fn is_unfunded_deploy(err: &anyhow::Error) -> bool {
    let text = format!("{err:?}").to_lowercase();
    text.contains("insufficient funds")
        || text.contains("insufficient phlo")
        || text.contains("not enough phlo")
}

#[derive(Debug, ApiResponse)]
pub enum PrepareCreateGrantResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<CreateGrantResp>>),
    /// The request named a channel, authority class, or cap outside the legal
    /// vocabulary (`grant_invalid_request`).
    #[oai(status = 422)]
    UnprocessableEntity(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<GrantError> for PrepareCreateGrantResponse {
    fn from(error: GrantError) -> Self {
        match error {
            GrantError::InvalidRequest(reason) => {
                Self::UnprocessableEntity(Json(ErrorBody::new("grant_invalid_request", reason)))
            }
            GrantError::NotFound | GrantError::Unauthorized => {
                Self::InternalError(Json(ErrorBody::new(
                    "internal_error",
                    "unexpected claim-phase failure during prepare",
                )))
            }
            GrantError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum SendGrantResponse {
    #[oai(status = 200)]
    Ok(Json<crate::api::common::SendResp>),
    /// The grant does not exist (`grant_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    /// The signer does not hold grant authority (`grant_unauthorized`).
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<GrantError> for SendGrantResponse {
    fn from(error: GrantError) -> Self {
        match error {
            GrantError::NotFound => {
                Self::NotFound(Json(ErrorBody::new("grant_not_found", "grant not found")))
            }
            GrantError::Unauthorized => Self::Forbidden(Json(ErrorBody::new(
                "grant_unauthorized",
                "the signer does not hold grant authority",
            ))),
            GrantError::InvalidRequest(reason) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", reason)))
            }
            GrantError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum PrepareRevokeGrantResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<RevokeGrantResp>>),
    /// The grant does not exist (`grant_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<GrantError> for PrepareRevokeGrantResponse {
    fn from(error: GrantError) -> Self {
        match error {
            GrantError::NotFound => {
                Self::NotFound(Json(ErrorBody::new("grant_not_found", "grant not found")))
            }
            GrantError::Unauthorized | GrantError::InvalidRequest(_) => {
                Self::InternalError(Json(ErrorBody::new(
                    "internal_error",
                    "unexpected claim-phase failure during prepare",
                )))
            }
            GrantError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum PrepareRunUnderGrantResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<RunUnderGrantResp>>),
    /// The grant does not exist (`grant_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    /// The grant is revoked (`grant_revoked`).
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    /// The channel context does not match the grant
    /// (`grant_principal_mismatch`), or the granted team has no durable
    /// deployment (`team_not_durable`).
    #[oai(status = 422)]
    UnprocessableEntity(Json<ErrorBody>),
    /// The grant's daily run cap is exhausted (`grant_caps_exceeded`).
    #[oai(status = 429)]
    TooManyRequests(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<RunUnderGrantError> for PrepareRunUnderGrantResponse {
    fn from(error: RunUnderGrantError) -> Self {
        match error {
            RunUnderGrantError::GrantNotFound => {
                Self::NotFound(Json(ErrorBody::new("grant_not_found", "grant not found")))
            }
            RunUnderGrantError::GrantRevoked => {
                Self::Forbidden(Json(ErrorBody::new("grant_revoked", "grant is revoked")))
            }
            RunUnderGrantError::PrincipalMismatch => {
                Self::UnprocessableEntity(Json(ErrorBody::new(
                    "grant_principal_mismatch",
                    "channel context does not match the grant's principal",
                )))
            }
            RunUnderGrantError::TeamNotDurable => Self::UnprocessableEntity(Json(ErrorBody::new(
                "team_not_durable",
                "the granted team has no durable workflow deployment",
            ))),
            RunUnderGrantError::GrantCapsExceeded => Self::TooManyRequests(Json(ErrorBody::new(
                "grant_caps_exceeded",
                "grant daily run cap exhausted",
            ))),
            RunUnderGrantError::NotFinalized(_) => Self::InternalError(Json(ErrorBody::new(
                "internal_error",
                "unexpected claim-phase failure during prepare",
            ))),
            RunUnderGrantError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum SendRunUnderGrantResponse {
    /// The run-state JSON read back from the observer after chain finality,
    /// exactly as the plain run send returns it.
    #[oai(status = 200)]
    Ok(Json<serde_json::Value>),
    /// The grant does not exist (`grant_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    /// The grant is revoked (`grant_revoked`).
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    /// The grant's daily run cap is exhausted (`grant_caps_exceeded`).
    #[oai(status = 429)]
    TooManyRequests(Json<ErrorBody>),
    /// The channel wallet could not fund the deploy (`deploy_unfunded`).
    #[oai(status = 402)]
    PaymentRequired(Json<ErrorBody>),
    /// The deploy was accepted but finality was not observed in time
    /// (`deploy_not_finalized`); the run may still complete -- poll the run
    /// state.
    #[oai(status = 504)]
    GatewayTimeout(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<RunUnderGrantError> for SendRunUnderGrantResponse {
    fn from(error: RunUnderGrantError) -> Self {
        match error {
            RunUnderGrantError::GrantNotFound => {
                Self::NotFound(Json(ErrorBody::new("grant_not_found", "grant not found")))
            }
            RunUnderGrantError::GrantRevoked => {
                Self::Forbidden(Json(ErrorBody::new("grant_revoked", "grant is revoked")))
            }
            RunUnderGrantError::GrantCapsExceeded => Self::TooManyRequests(Json(ErrorBody::new(
                "grant_caps_exceeded",
                "grant daily run cap exhausted",
            ))),
            RunUnderGrantError::PrincipalMismatch | RunUnderGrantError::TeamNotDurable => {
                Self::InternalError(Json(ErrorBody::new(
                    "internal_error",
                    "prepare-phase validation failed during send",
                )))
            }
            RunUnderGrantError::NotFinalized(deploy_id) => {
                Self::GatewayTimeout(Json(ErrorBody::new(
                    "deploy_not_finalized",
                    format!("run deploy {deploy_id} was not finalized; poll the run state"),
                )))
            }
            RunUnderGrantError::Internal(err) if is_unfunded_deploy(&err) => {
                Self::PaymentRequired(Json(ErrorBody::new("deploy_unfunded", format!("{err:?}"))))
            }
            RunUnderGrantError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum PreparePublishToMailResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<PublishToMailResp>>),
    /// The team does not exist under the address (`team_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    /// The address is already actively bound to a different team
    /// (`binding_duplicate`).
    #[oai(status = 409)]
    Conflict(Json<ErrorBody>),
    /// The address fails the grammar (`invalid_mail_address`), or the team
    /// has no deployment to bind (`team_not_deployed`).
    #[oai(status = 422)]
    UnprocessableEntity(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<PublishToMailError> for PreparePublishToMailResponse {
    fn from(error: PublishToMailError) -> Self {
        match error {
            PublishToMailError::NotFound => Self::NotFound(Json(ErrorBody::new(
                "team_not_found",
                "the team does not exist under the address",
            ))),
            PublishToMailError::BindingDuplicate => Self::Conflict(Json(ErrorBody::new(
                "binding_duplicate",
                "the mail address is already bound to another team",
            ))),
            PublishToMailError::InvalidAddress(reason) => {
                Self::UnprocessableEntity(Json(ErrorBody::new("invalid_mail_address", reason)))
            }
            PublishToMailError::NotDeployed => Self::UnprocessableEntity(Json(ErrorBody::new(
                "team_not_deployed",
                "the team has no deployment to bind",
            ))),
            PublishToMailError::Unauthorized => Self::InternalError(Json(ErrorBody::new(
                "internal_error",
                "unexpected claim-phase failure during prepare",
            ))),
            PublishToMailError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum PrepareUnpublishMailResponse {
    #[oai(status = 200)]
    Ok(Json<PrepareResponse<UnpublishMailResp>>),
    /// No active mail binding exists for the team (`binding_not_found`).
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<PublishToMailError> for PrepareUnpublishMailResponse {
    fn from(error: PublishToMailError) -> Self {
        match error {
            PublishToMailError::NotFound => Self::NotFound(Json(ErrorBody::new(
                "binding_not_found",
                "no active mail binding exists for the team",
            ))),
            PublishToMailError::BindingDuplicate
            | PublishToMailError::InvalidAddress(_)
            | PublishToMailError::NotDeployed
            | PublishToMailError::Unauthorized => Self::InternalError(Json(ErrorBody::new(
                "internal_error",
                "unexpected claim-phase failure during prepare",
            ))),
            PublishToMailError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, ApiResponse)]
pub enum SendPublishToMailResponse {
    #[oai(status = 200)]
    Ok(Json<crate::api::common::SendResp>),
    /// The address is already actively bound to a different team
    /// (`binding_duplicate`).
    #[oai(status = 409)]
    Conflict(Json<ErrorBody>),
    /// The signer does not own the team (`binding_unauthorized`).
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    /// No active binding exists (`binding_not_found`); unpublish only.
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}

impl From<PublishToMailError> for SendPublishToMailResponse {
    fn from(error: PublishToMailError) -> Self {
        match error {
            PublishToMailError::BindingDuplicate => Self::Conflict(Json(ErrorBody::new(
                "binding_duplicate",
                "the mail address is already bound to another team",
            ))),
            PublishToMailError::Unauthorized => Self::Forbidden(Json(ErrorBody::new(
                "binding_unauthorized",
                "the signer does not own the team",
            ))),
            PublishToMailError::NotFound => Self::NotFound(Json(ErrorBody::new(
                "binding_not_found",
                "no active mail binding exists for the team",
            ))),
            PublishToMailError::NotDeployed | PublishToMailError::InvalidAddress(_) => {
                Self::InternalError(Json(ErrorBody::new(
                    "internal_error",
                    "prepare-phase validation failed during send",
                )))
            }
            PublishToMailError::Internal(err) => {
                Self::InternalError(Json(ErrorBody::new("internal_error", format!("{err:?}"))))
            }
        }
    }
}

#[derive(Debug, Clone, Hash, Object)]
pub struct TracePrepareResponse {
    pub contract: PreparedContract,
    pub nonce: String,
    pub expires_at: i64,
}
impl From<crate::domain::agents_teams::trace::TracePreparation> for TracePrepareResponse {
    fn from(value: crate::domain::agents_teams::trace::TracePreparation) -> Self {
        Self {
            contract: value.contract.into(),
            nonce: value.nonce,
            expires_at: value.expires_at,
        }
    }
}
#[derive(Debug, ApiResponse)]
pub enum TraceResponse<T: poem_openapi::types::Type + poem_openapi::types::ToJSON + Send + Sync> {
    #[oai(status = 200)]
    Ok(Json<T>),
    #[oai(status = 403)]
    Forbidden(Json<ErrorBody>),
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 410)]
    Expired(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<ErrorBody>),
}
impl<T: poem_openapi::types::Type + poem_openapi::types::ToJSON + Send + Sync>
    From<crate::domain::agents_teams::trace::TraceError> for TraceResponse<T>
{
    fn from(error: crate::domain::agents_teams::trace::TraceError) -> Self {
        use crate::domain::agents_teams::trace::TraceError;
        let body = ErrorBody::new("trace_error", &error);
        match error {
            TraceError::Unauthorized => Self::Forbidden(Json(body)),
            TraceError::NotFound => Self::NotFound(Json(body)),
            TraceError::Expired => Self::Expired(Json(body)),
            TraceError::Internal(_) => Self::InternalError(Json(body)),
        }
    }
}
