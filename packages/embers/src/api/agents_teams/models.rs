use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};
use firefly_client::models::{Uri, WalletAddress};
use poem_openapi::registry::{MetaSchema, MetaSchemaRef};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, RegistryDeploy, SignedContract, Stringified};
use crate::domain::agents_teams::models;

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
    pub definition: Option<WorkflowDefinition>,
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
    pub definition: Option<WorkflowDefinition>,
}

/// A team whose latest version uses a given agent, and would break if it were deleted.
#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::ReferencingTeam))]
pub struct ReferencingTeam {
    pub id: String,
    pub name: String,
    pub version: String,
    pub published: bool,
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
    pub handle: Option<String>,
    pub did: Option<String>,
    pub account_exists: Option<bool>,
    pub active: Option<bool>,
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
    pub deploy: RegistryDeploy,
}

#[derive(Debug, Clone, Hash, Object)]
pub struct DeployGraph {
    pub graph: Stringified<models::Graph>,
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
    pub agents_team: Stringified<Uri>,
}

#[derive(Debug, Clone, Hash, StructuralConvert, Object)]
#[convert(from(models::RunResp))]
pub struct RunResp {
    pub run_id: String,
    pub contract: PreparedContract,
    pub created_at: Stringified<DateTime<Utc>>,
    pub durable: bool,
    pub definition_hash: Option<String>,
    pub request_content_hash: String,
    pub request_capability_hash: String,
    pub continuation_content_hash: Option<String>,
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
    pub expected_revision: Stringified<u64>,
    #[oai(rename = "type")]
    pub command_type: RunCommandKind,
    pub wait_id: Option<String>,
    pub name: Option<String>,
    pub payload: Option<JsonValue>,
    pub decision_id: Option<String>,
    pub expected_deadline: Option<Stringified<DateTime<Utc>>>,
    pub reason: Option<String>,
    pub from_definition_hash: Option<String>,
    pub to_definition_hash: Option<String>,
    pub target_team_uri: Option<String>,
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
    pub command_id: String,
    pub command_class: String,
    pub target_wait_id: Option<String>,
    pub contract: PreparedContract,
    pub issued_at: Stringified<DateTime<Utc>>,
    pub command_content_hash: String,
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
    pub run_id: String,
    pub agents_team: Stringified<Uri>,
    pub created_at: Stringified<DateTime<Utc>>,
    pub status: String,
    pub events: Vec<RunEvent>,
    pub definition_hash: Option<String>,
    pub revision: Stringified<u64>,
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
    pub due_at: Option<Stringified<DateTime<Utc>>>,
    pub output_content_hash: Option<String>,
    pub output_capability_hash: Option<String>,
}

#[derive(Debug, Clone, StructuralConvert, Object)]
#[convert(from(models::RunEvent))]
pub struct RunEvent {
    pub sequence: Stringified<u64>,
    pub event_type: String,
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
