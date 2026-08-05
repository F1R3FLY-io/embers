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
use crate::domain::agents_teams::{RevealOutputError, models};

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
