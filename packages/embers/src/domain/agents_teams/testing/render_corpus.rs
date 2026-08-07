//! Renders every Rholang template in the crate with representative,
//! deterministic fixture data and writes the results to
//! `target/rendered-corpus/<name>.rho`, one file per template (the six
//! per-node team templates are exercised through the compiled team deploy,
//! whose output embeds each of them; `common/insert_signed.rho` is the base
//! template every `init.rho` extends). A later verification step feeds this
//! corpus to a Rholang-parser oracle.
//!
//! Most production `Render` structs are private to their defining modules,
//! so this test re-declares byte-equivalent mirrors: the derive expands each
//! `#[template(path = …)]` struct against the same template file with the
//! same field names, so a mirror render is identical to the production
//! render for identical field values. The two structs this module can reach
//! (the agents-teams env initializers, defined in the parent module) are
//! rendered directly; the compiled team and compiled workflow go through the
//! real `compilation::render` / `compilation::render_workflow` entry points.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, Uri, WalletAddress};
use firefly_client::rendering::{Inline, Render, Value as RhoValue};
use serde_json::json;
use uuid::Uuid;

use crate::domain::agents_teams::compilation::{self, AgentRef};
use crate::domain::agents_teams::models::Graph;
use crate::domain::agents_teams::workflow::WorkflowDefinition;
use crate::domain::agents_teams::{GetFireskyTokens, InitAgentsTeamsEnv};
use crate::domain::common::RegistryDeploy;
use crate::domain::test_helpers::{test_deploy_id, test_public_key, test_uri};

// -------------------------------------------------------
// Deterministic fixtures
// -------------------------------------------------------

fn fixture_uri() -> Uri {
    test_uri()
}

fn fixture_address() -> WalletAddress {
    test_public_key().into()
}

fn fixture_timestamp() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

fn fixture_uuid() -> Uuid {
    Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef)
}

fn fixture_deploy_id() -> DeployId {
    test_deploy_id()
}

fn fixture_registry_deploy() -> RegistryDeploy {
    RegistryDeploy {
        timestamp: fixture_timestamp(),
        version: 1,
        uri_pub_key: test_public_key(),
        signature: vec![1, 2, 3, 4],
    }
}

// -------------------------------------------------------
// Mirrors of the private production Render structs
// -------------------------------------------------------

#[derive(Debug, Clone, Render)]
#[template(path = "agents/init.rho", blocks = ["name"])]
struct AgentsInit {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/create.rho")]
struct AgentsCreate {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/delete.rho")]
struct AgentsDelete {
    env_uri: Uri,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/get.rho")]
struct AgentsGet {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/list.rho")]
struct AgentsList {
    env_uri: Uri,
    address: WalletAddress,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/list_versions.rho")]
struct AgentsListVersions {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/save.rho")]
struct AgentsSave {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/record_deploy.rho")]
struct AgentsRecordDeploy {
    env_uri: Uri,
    id: String,
    version: String,
    last_deploy: DateTime<Utc>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents/deploy_agent.rho")]
struct AgentsDeployAgent {
    code: Inline,
    handle: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/create.rho")]
struct TeamsCreate {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    graph: Option<String>,
    definition: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save.rho")]
struct TeamsSave {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    shard: Option<String>,
    logo: Option<String>,
    graph: Option<String>,
    definition: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/delete.rho")]
struct TeamsDelete {
    env_uri: Uri,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get.rho")]
struct TeamsGet {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/list.rho")]
struct TeamsList {
    env_uri: Uri,
    address: WalletAddress,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/list_versions.rho")]
struct TeamsListVersions {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/record_deploy.rho")]
struct TeamsRecordDeploy {
    env_uri: Uri,
    id: String,
    version: String,
    last_deploy: DateTime<Utc>,
    uri: Uri,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/run.rho")]
struct TeamsRun {
    env_uri: Uri,
    agents_team: Uri,
    prompt: String,
    run_id: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run_result.rho")]
struct TeamsGetRunResult {
    deploy_id: DeployId,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_manifest.rho")]
struct TeamsGetManifest {
    agents_team: Uri,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/begin_durable_run.rho")]
struct TeamsBeginDurableRun {
    env_uri: Uri,
    agents_team: Uri,
    run_id: Uuid,
    created_at: DateTime<Utc>,
    definition_hash: String,
    request_content_hash: String,
    request_capability_hash: String,
    continuation_content_hash: String,
    continuation_capability_hash: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/claim_command.rho")]
struct TeamsClaimCommand {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    command_id: String,
    command_class: String,
    target_wait_id: Option<String>,
    command_content_hash: String,
    command_capability_hash: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/clear_command.rho")]
struct TeamsClearCommand {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    command_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/advance_run.rho")]
struct TeamsAdvanceRun {
    env_uri: Uri,
    run_id: String,
    expected_revision: i64,
    status: String,
    definition_hash: String,
    continuation_content_hash: String,
    continuation_capability_hash: String,
    wait_ids: Vec<String>,
    due_at: Option<DateTime<Utc>>,
    approval_authority_grants: Vec<String>,
    output_content_hash: Option<String>,
    output_capability_hash: Option<String>,
    event_type: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/invoke_activity.rho")]
struct TeamsInvokeActivity {
    agents_team: Uri,
    node_id: String,
    input: RhoValue,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/invoke_guard.rho")]
struct TeamsInvokeGuard {
    agents_team: Uri,
    transition_id: String,
    value: RhoValue,
    input: RhoValue,
    state: RhoValue,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/claim_output_reveal.rho")]
struct TeamsClaimOutputReveal {
    env_uri: Uri,
    run_id: String,
    reveal_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run.rho")]
struct TeamsGetRun {
    env_uri: Uri,
    run_id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/list_runs.rho")]
struct TeamsListRuns {
    env_uri: Uri,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/save_firesky_token.rho")]
struct TeamsSaveFireskyToken {
    env_uri: Uri,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/init.rho", blocks = ["name"])]
struct OslfsInit {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/create.rho")]
struct OslfsCreate {
    env_uri: Uri,
    id: Uuid,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    query: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/delete.rho")]
struct OslfsDelete {
    env_uri: Uri,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/get.rho")]
struct OslfsGet {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/list.rho")]
struct OslfsList {
    env_uri: Uri,
    address: WalletAddress,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/list_versions.rho")]
struct OslfsListVersions {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "oslfs/save.rho")]
struct OslfsSave {
    env_uri: Uri,
    id: String,
    version: Uuid,
    created_at: DateTime<Utc>,
    name: String,
    description: Option<String>,
    query: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/init.rho", blocks = ["name"])]
struct TestnetInit {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/fund_test_wallet.rho")]
struct TestnetFundTestWallet {
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
}

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/get_logs.rho")]
struct TestnetGetLogs {
    env_uri: Uri,
    deploy_id: DeployId,
}

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/init.rho", blocks = ["name"])]
struct WalletsInit {
    env_uri: Uri,
    version: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/get_balance_and_history.rho")]
struct WalletsGetBalanceAndHistory {
    env_uri: Uri,
    wallet_address: WalletAddress,
}

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/transfer.rho")]
struct WalletsTransfer {
    env_uri: Uri,
    timestamp: DateTime<Utc>,
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
    description: Option<String>,
}

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/boost.rho")]
struct WalletsBoost {
    env_uri: Uri,
    timestamp: DateTime<Utc>,
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
    description: Option<String>,
    post_author_did: String,
    post_id: Option<String>,
}

// -------------------------------------------------------
// Compiled-team fixtures (real compilation entry points)
// -------------------------------------------------------

/// `GraphL` in the editor's emission shape: one of every node kind, wired
/// input → models → compress/agent → output.
fn corpus_graphl() -> String {
    let nodes: [(&str, String); 7] = [
        ("aIn", r#"{"type":"input-node"}"#.to_owned()),
        ("aText", r#"{"type":"text-model"}"#.to_owned()),
        ("aTti", r#"{"type":"tti-model"}"#.to_owned()),
        ("aTts", r#"{"type":"tts-model"}"#.to_owned()),
        ("aCompress", r#"{"type":"compress"}"#.to_owned()),
        (
            "aAgent",
            r#"{"type":"agent","agentId":"agent-1","agentName":"Summarize","agentVersion":"v1"}"#
                .to_owned(),
        ),
        ("aOut", r#"{"type":"output-node"}"#.to_owned()),
    ];
    let edges = [
        ("aIn", "aText"),
        ("aIn", "aTti"),
        ("aIn", "aTts"),
        ("aText", "aCompress"),
        ("aTts", "aCompress"),
        ("aCompress", "aAgent"),
        ("aTti", "aAgent"),
        ("aAgent", "aOut"),
    ];

    let mut body = "0".to_owned();
    for (name, json) in nodes.iter().rev() {
        let escaped = json.replace('"', "\\\"");
        body = format!(
            "{{context \"{escaped}\" for {name} in let {name} = < {name} > in < {name} > | {body}}}"
        );
    }
    let mut edge_chain = "0".to_owned();
    for (from, to) in edges.iter().rev() {
        edge_chain =
            format!("{{(let {from} = < {from} > in 0, let {to} = < {to} > in 0) * {edge_chain}}}");
    }
    format!("0 * {body} * {edge_chain}")
}

fn corpus_agent_codes() -> compilation::AgentCodes {
    compilation::AgentCodes::from([(
        AgentRef {
            id: "agent-1".to_owned(),
            version: "v1".to_owned(),
        },
        "contract main(@input, return) = { return!(input) }".to_owned(),
    )])
}

/// Renders the legacy `GraphL` lowering: `agents_teams/deploy.rho` plus all six
/// `agents_teams/nodes/*.rho` templates, whose rendered bodies are embedded
/// in the team output.
fn compiled_team() -> String {
    let graph = Graph::new(corpus_graphl()).expect("corpus GraphL parses");
    let code = compilation::parse(&graph).expect("corpus GraphL lowers");
    assert_eq!(
        code.agent_refs().len(),
        1,
        "the corpus graph references exactly one agent"
    );
    compilation::render(code, fixture_registry_deploy(), &corpus_agent_codes())
        .expect("the corpus team renders")
}

/// A publishable Workflow Definition v2 exercising the workflow lowering:
/// an agent activity, a builtin model, a registry Rholang activity, a timer
/// wait, an approval, an all-join, a bounded self-loop, and a pure Rholang
/// guard.
fn corpus_workflow() -> WorkflowDefinition {
    let value = json!({
        "schemaVersion": "2.0",
        "entryScopeId": "root",
        "interface": {
            "inputs": {"input": {"kind": "legacy"}},
            "outputs": {"output": {"kind": "legacy"}}
        },
        "nodes": {
            "in": {"id": "in", "label": "Input", "scopeId": "root", "kind": "input"},
            "draft": {"id": "draft", "label": "Draft", "scopeId": "root", "kind": "activity",
                      "activity": {"kind": "agent", "agentId": "agent-1",
                                   "agentVersion": "v1", "protocol": "structured-v1"}},
            "model": {"id": "model", "label": "Model", "scopeId": "root", "kind": "activity",
                      "activity": {"kind": "builtin", "name": "text-model"}},
            "join": {"id": "join", "label": "Join", "scopeId": "root",
                     "join": {"mode": "all", "correlation": {"kind": "root"}},
                     "kind": "gateway"},
            "gate": {"id": "gate", "label": "Gate", "scopeId": "root", "kind": "approval",
                     "policyId": "owner", "quorum": 1, "approvers": []},
            "wait": {"id": "wait", "label": "Cooldown", "scopeId": "root", "kind": "wait",
                     "waitFor": "timer", "timer": {"delayMillis": 1000}},
            "ext": {"id": "ext", "label": "External", "scopeId": "root", "kind": "activity",
                    "activity": {"kind": "rholang", "protocol": "simple-v1",
                                 "uri": "rho:id:external-activity"}},
            "out": {"id": "out", "label": "Output", "scopeId": "root", "kind": "output"}
        },
        "policies": {"defaultExecution": "durable", "visibility": "private"},
        "presentation": {"edges": {}, "nodes": {}},
        "routeGroups": {
            "fork": {"id": "fork", "mode": "allMatches", "noMatch": "fail"},
            "draft-done": {"id": "draft-done", "mode": "allMatches", "noMatch": "fail"},
            "model-done": {"id": "model-done", "mode": "allMatches", "noMatch": "fail"},
            "model-loop": {"id": "model-loop", "mode": "allMatches", "noMatch": "skip"},
            "joined": {"id": "joined", "mode": "allMatches", "noMatch": "fail"},
            "approved": {"id": "approved", "mode": "allMatches", "noMatch": "fail"},
            "rested": {"id": "rested", "mode": "allMatches", "noMatch": "fail"},
            "done": {"id": "done", "mode": "allMatches", "noMatch": "fail"}
        },
        "scopes": {
            "root": {"id": "root", "label": "Root", "kind": "root",
                     "initialNodeIds": ["in"], "entryActivities": [],
                     "exitActivities": [], "history": "none"}
        },
        "transitions": {
            "to-draft": {"id": "to-draft", "sourceNodeId": "in", "targetNodeId": "draft",
                         "trigger": "completed", "priority": 0, "routeGroupId": "fork"},
            "to-model": {"id": "to-model", "sourceNodeId": "in", "targetNodeId": "model",
                         "trigger": "completed", "priority": 1, "routeGroupId": "fork"},
            "draft-join": {"id": "draft-join", "sourceNodeId": "draft", "targetNodeId": "join",
                           "trigger": "completed", "priority": 0, "routeGroupId": "draft-done"},
            "model-join": {"id": "model-join", "sourceNodeId": "model", "targetNodeId": "join",
                           "trigger": "completed", "priority": 0, "routeGroupId": "model-done"},
            "model-again": {"id": "model-again", "sourceNodeId": "model", "targetNodeId": "model",
                            "trigger": "completed", "priority": 0, "routeGroupId": "model-loop",
                            "loop": {"maxIterations": 2},
                            "guard": {"kind": "literal", "value": false}},
            "join-gate": {"id": "join-gate", "sourceNodeId": "join", "targetNodeId": "gate",
                          "trigger": "completed", "priority": 0, "routeGroupId": "joined",
                          "guard": {"kind": "rholang",
                                    "inputs": {"released": {"root": "output", "path": []}},
                                    "source": "$released == $released"}},
            "gate-wait": {"id": "gate-wait", "sourceNodeId": "gate", "targetNodeId": "wait",
                          "trigger": "completed", "priority": 0, "routeGroupId": "approved"},
            "wait-ext": {"id": "wait-ext", "sourceNodeId": "wait", "targetNodeId": "ext",
                         "trigger": "completed", "priority": 0, "routeGroupId": "rested"},
            "ext-out": {"id": "ext-out", "sourceNodeId": "ext", "targetNodeId": "out",
                        "trigger": "completed", "priority": 0, "routeGroupId": "done"}
        },
        "types": {}
    });
    let workflow = WorkflowDefinition::new(value).expect("corpus workflow parses");
    workflow
        .parsed()
        .validate_publishable()
        .expect("corpus workflow is publishable");
    workflow
}

fn compiled_workflow() -> String {
    compilation::render_workflow(
        &corpus_workflow(),
        fixture_registry_deploy(),
        &corpus_agent_codes(),
    )
    .expect("the corpus workflow renders")
}

// -------------------------------------------------------
// The corpus writer
// -------------------------------------------------------

fn corpus_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
        },
        PathBuf::from,
    )
    .join("rendered-corpus")
}

fn rho_map(entries: &[(&str, &str)]) -> RhoValue {
    RhoValue::Map(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), RhoValue::String((*value).to_owned())))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn render_corpus_writes_all_templates() {
    let hash = "0".repeat(64);
    let mut corpus: Vec<(&str, String)> = Vec::with_capacity(47);
    let mut push = |name: &'static str, rendered: Result<String, askama::Error>| {
        let rendered = rendered.unwrap_or_else(|error| panic!("{name} must render: {error}"));
        assert!(
            !rendered.trim().is_empty(),
            "{name} must render non-empty Rholang"
        );
        corpus.push((name, rendered));
    };

    // agents
    push(
        "agents_init",
        AgentsInit {
            env_uri: fixture_uri(),
            version: 1,
            public_key: test_public_key().serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render(),
    );
    push(
        "agents_create",
        AgentsCreate {
            env_uri: fixture_uri(),
            id: fixture_uuid(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus agent".into(),
            description: Some("An agent fixture".into()),
            shard: Some("shard-1".into()),
            logo: None,
            code: Some("contract main(@input, return) = { return!(input) }".into()),
        }
        .render(),
    );
    push(
        "agents_delete",
        AgentsDelete {
            env_uri: fixture_uri(),
            id: "agent-1".into(),
        }
        .render(),
    );
    push(
        "agents_get",
        AgentsGet {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "agent-1".into(),
            version: "v1".into(),
        }
        .render(),
    );
    push(
        "agents_list",
        AgentsList {
            env_uri: fixture_uri(),
            address: fixture_address(),
        }
        .render(),
    );
    push(
        "agents_list_versions",
        AgentsListVersions {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "agent-1".into(),
        }
        .render(),
    );
    push(
        "agents_save",
        AgentsSave {
            env_uri: fixture_uri(),
            id: "agent-1".into(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus agent".into(),
            description: None,
            shard: None,
            logo: Some("logo.png".into()),
            code: None,
        }
        .render(),
    );
    push(
        "agents_record_deploy",
        AgentsRecordDeploy {
            env_uri: fixture_uri(),
            id: "agent-1".into(),
            version: "v1".into(),
            last_deploy: fixture_timestamp(),
        }
        .render(),
    );
    push(
        "agents_deploy_agent",
        AgentsDeployAgent {
            code: Inline::from(
                "contract main(@input, return) = { return!(input) }".to_owned(),
            ),
            handle: "agent:agent-1:v1".into(),
        }
        .render(),
    );

    // agents_teams — the two env structs are the real production structs.
    push(
        "agents_teams_init",
        InitAgentsTeamsEnv {
            env_uri: fixture_uri(),
            version: 2,
            public_key: test_public_key().serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render(),
    );
    push(
        "agents_teams_get_firesky_tokens",
        GetFireskyTokens {
            env_uri: fixture_uri(),
        }
        .render(),
    );
    push(
        "agents_teams_create",
        TeamsCreate {
            env_uri: fixture_uri(),
            id: fixture_uuid(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus team".into(),
            description: Some("A team fixture".into()),
            shard: None,
            logo: None,
            graph: Some(corpus_graphl()),
            definition: Some(corpus_workflow().json()),
        }
        .render(),
    );
    push(
        "agents_teams_save",
        TeamsSave {
            env_uri: fixture_uri(),
            id: "team-1".into(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus team".into(),
            description: None,
            shard: Some("shard-1".into()),
            logo: Some("logo.png".into()),
            graph: None,
            definition: Some(corpus_workflow().json()),
        }
        .render(),
    );
    push(
        "agents_teams_delete",
        TeamsDelete {
            env_uri: fixture_uri(),
            id: "team-1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_get",
        TeamsGet {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "team-1".into(),
            version: "v1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_list",
        TeamsList {
            env_uri: fixture_uri(),
            address: fixture_address(),
        }
        .render(),
    );
    push(
        "agents_teams_list_versions",
        TeamsListVersions {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "team-1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_record_deploy",
        TeamsRecordDeploy {
            env_uri: fixture_uri(),
            id: "team-1".into(),
            version: "v1".into(),
            last_deploy: fixture_timestamp(),
            uri: fixture_uri(),
        }
        .render(),
    );
    push(
        "agents_teams_run",
        TeamsRun {
            env_uri: fixture_uri(),
            agents_team: fixture_uri(),
            prompt: "Summarize the corpus".into(),
            run_id: fixture_uuid(),
            created_at: fixture_timestamp(),
        }
        .render(),
    );
    push(
        "agents_teams_get_run_result",
        TeamsGetRunResult {
            deploy_id: fixture_deploy_id(),
        }
        .render(),
    );
    push(
        "agents_teams_get_manifest",
        TeamsGetManifest {
            agents_team: fixture_uri(),
        }
        .render(),
    );
    push(
        "agents_teams_begin_durable_run",
        TeamsBeginDurableRun {
            env_uri: fixture_uri(),
            agents_team: fixture_uri(),
            run_id: fixture_uuid(),
            created_at: fixture_timestamp(),
            definition_hash: hash.clone(),
            request_content_hash: hash.clone(),
            request_capability_hash: hash.clone(),
            continuation_content_hash: hash.clone(),
            continuation_capability_hash: hash.clone(),
        }
        .render(),
    );
    push(
        "agents_teams_claim_command",
        TeamsClaimCommand {
            env_uri: fixture_uri(),
            run_id: "run-1".into(),
            expected_revision: 3,
            command_id: "command-1".into(),
            command_class: "approve".into(),
            target_wait_id: Some("approval:token:0:gate".into()),
            command_content_hash: hash.clone(),
            command_capability_hash: hash.clone(),
        }
        .render(),
    );
    push(
        "agents_teams_clear_command",
        TeamsClearCommand {
            env_uri: fixture_uri(),
            run_id: "run-1".into(),
            expected_revision: 3,
            command_id: "command-1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_advance_run",
        TeamsAdvanceRun {
            env_uri: fixture_uri(),
            run_id: "run-1".into(),
            expected_revision: 4,
            status: "suspended".into(),
            definition_hash: hash.clone(),
            continuation_content_hash: hash.clone(),
            continuation_capability_hash: hash,
            wait_ids: vec!["wait:token:0:wait".into()],
            due_at: Some(fixture_timestamp()),
            approval_authority_grants: vec!["alice".into()],
            output_content_hash: None,
            output_capability_hash: None,
            event_type: "runSuspended".into(),
        }
        .render(),
    );
    push(
        "agents_teams_invoke_activity",
        TeamsInvokeActivity {
            agents_team: fixture_uri(),
            node_id: "draft".into(),
            input: rho_map(&[("prompt", "corpus")]),
        }
        .render(),
    );
    push(
        "agents_teams_invoke_guard",
        TeamsInvokeGuard {
            agents_team: fixture_uri(),
            transition_id: "join-gate".into(),
            value: RhoValue::String("released".into()),
            input: rho_map(&[("prompt", "corpus")]),
            state: RhoValue::Map(BTreeMap::new()),
        }
        .render(),
    );
    push(
        "agents_teams_claim_output_reveal",
        TeamsClaimOutputReveal {
            env_uri: fixture_uri(),
            run_id: "run-1".into(),
            reveal_id: "reveal-1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_get_run",
        TeamsGetRun {
            env_uri: fixture_uri(),
            run_id: "run-1".into(),
        }
        .render(),
    );
    push(
        "agents_teams_list_runs",
        TeamsListRuns {
            env_uri: fixture_uri(),
        }
        .render(),
    );
    push(
        "agents_teams_save_firesky_token",
        TeamsSaveFireskyToken {
            env_uri: fixture_uri(),
            nonce: vec![9; 12],
            ciphertext: vec![7; 32],
        }
        .render(),
    );
    push("agents_teams_deploy", Ok(compiled_team()));
    push("agents_teams_deploy_workflow", Ok(compiled_workflow()));

    // oslfs
    push(
        "oslfs_init",
        OslfsInit {
            env_uri: fixture_uri(),
            version: 1,
            public_key: test_public_key().serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render(),
    );
    push(
        "oslfs_create",
        OslfsCreate {
            env_uri: fixture_uri(),
            id: fixture_uuid(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus oslf".into(),
            description: Some("An OSLF fixture".into()),
            query: Some("select *".into()),
        }
        .render(),
    );
    push(
        "oslfs_delete",
        OslfsDelete {
            env_uri: fixture_uri(),
            id: "oslf-1".into(),
        }
        .render(),
    );
    push(
        "oslfs_get",
        OslfsGet {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "oslf-1".into(),
            version: "v1".into(),
        }
        .render(),
    );
    push(
        "oslfs_list",
        OslfsList {
            env_uri: fixture_uri(),
            address: fixture_address(),
        }
        .render(),
    );
    push(
        "oslfs_list_versions",
        OslfsListVersions {
            env_uri: fixture_uri(),
            address: fixture_address(),
            id: "oslf-1".into(),
        }
        .render(),
    );
    push(
        "oslfs_save",
        OslfsSave {
            env_uri: fixture_uri(),
            id: "oslf-1".into(),
            version: fixture_uuid(),
            created_at: fixture_timestamp(),
            name: "Corpus oslf".into(),
            description: None,
            query: None,
        }
        .render(),
    );

    // testnet
    push(
        "testnet_init",
        TestnetInit {
            env_uri: fixture_uri(),
            version: 1,
            public_key: test_public_key().serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render(),
    );
    push(
        "testnet_fund_test_wallet",
        TestnetFundTestWallet {
            wallet_address_from: fixture_address(),
            wallet_address_to: fixture_address(),
            amount: 1_000_000_000,
        }
        .render(),
    );
    push(
        "testnet_get_logs",
        TestnetGetLogs {
            env_uri: fixture_uri(),
            deploy_id: fixture_deploy_id(),
        }
        .render(),
    );

    // wallets
    push(
        "wallets_init",
        WalletsInit {
            env_uri: fixture_uri(),
            version: 1,
            public_key: test_public_key().serialize_uncompressed().into(),
            sig: vec![1, 2, 3, 4],
        }
        .render(),
    );
    push(
        "wallets_get_balance_and_history",
        WalletsGetBalanceAndHistory {
            env_uri: fixture_uri(),
            wallet_address: fixture_address(),
        }
        .render(),
    );
    push(
        "wallets_transfer",
        WalletsTransfer {
            env_uri: fixture_uri(),
            timestamp: fixture_timestamp(),
            wallet_address_from: fixture_address(),
            wallet_address_to: fixture_address(),
            amount: 42,
            description: Some("corpus transfer".into()),
        }
        .render(),
    );
    push(
        "wallets_boost",
        WalletsBoost {
            env_uri: fixture_uri(),
            timestamp: fixture_timestamp(),
            wallet_address_from: fixture_address(),
            wallet_address_to: fixture_address(),
            amount: 7,
            description: None,
            post_author_did: "did:plc:corpus".into(),
            post_id: Some("post-1".into()),
        }
        .render(),
    );

    assert_eq!(
        corpus.len(),
        47,
        "the corpus covers every template: 9 agents, 24 agents-teams (including \
         the compiled team, which embeds the 6 node templates, and the compiled \
         workflow), 7 oslfs, 3 testnet, 4 wallets"
    );

    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).expect("corpus directory is creatable");
    for (name, rendered) in &corpus {
        std::fs::write(dir.join(format!("{name}.rho")), rendered)
            .unwrap_or_else(|error| panic!("writing {name}.rho must succeed: {error}"));
    }
}
