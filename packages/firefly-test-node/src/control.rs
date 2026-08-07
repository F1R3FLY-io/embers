//! Cloneable control handles that let a test reconfigure a running
//! [`FakeNode`](crate::FakeNode) and inject events after it has started.
//!
//! Every handle is an `Arc`-backed newtype, so cloning it is cheap and all
//! clones observe the same shared state. The handles are the only mutable
//! surface of the fake: the gRPC / HTTP / WebSocket tasks read from exactly the
//! same shared state the handles write to.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use firefly_client::models::rhoapi;
use secp256k1::PublicKey;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast};

/// Capacity of the WebSocket command broadcast ring. Injected events are
/// buffered here until each connected socket forwards them, so a test that
/// injects a burst before the client polls does not lose events.
const WS_BROADCAST_CAPACITY: usize = 256;

/// Lock a mutex, recovering the guard if a previous holder panicked. The fake's
/// state is a set of plain values, so a poisoned lock carries no torn invariant
/// worth propagating a panic for — recovering keeps the doubles responsive.
fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// Validator (gRPC) control
// ---------------------------------------------------------------------------

/// Outcome the fake validator returns from `doDeploy`.
#[derive(Clone, Debug)]
pub enum DeployOutcome {
    /// The deploy is accepted; the client receives `Success! DeployId is: {0}`
    /// and extracts the wrapped id.
    Accepted(String),
    /// The deploy is rejected; the wrapped strings become the gRPC
    /// `ServiceError.messages`, which the client surfaces as an error.
    Error(Vec<String>),
}

impl Default for DeployOutcome {
    fn default() -> Self {
        Self::Accepted("test-deploy-id".to_owned())
    }
}

/// A value the fake validator serves from `getDataAtName` for a channel.
///
/// Each variant is lowered into a single-expression `rhoapi::Par`, matching the
/// shape [`WriteNodeClient::get_channel_value`](firefly_client::WriteNodeClient)
/// expects (it reads the last `Par`, its first `Expr`, and decodes via
/// `FromExpr`).
#[derive(Clone, Debug)]
pub enum ChannelValue {
    String(String),
    Int(i64),
    Bool(bool),
    Bytes(Vec<u8>),
    /// Escape hatch for values the simple variants above cannot express (lists,
    /// maps, tuples). Supplied verbatim as the returned `Par`. Boxed because a
    /// `Par` dwarfs the other variants.
    Par(Box<rhoapi::Par>),
}

impl ChannelValue {
    fn into_par(self) -> rhoapi::Par {
        use rhoapi::expr::ExprInstance;

        let instance = match self {
            Self::String(value) => ExprInstance::GString(value),
            Self::Int(value) => ExprInstance::GInt(value),
            Self::Bool(value) => ExprInstance::GBool(value),
            Self::Bytes(value) => ExprInstance::GByteArray(value),
            Self::Par(par) => return *par,
        };
        rhoapi::Par {
            exprs: vec![rhoapi::Expr {
                expr_instance: Some(instance),
            }],
            ..Default::default()
        }
    }
}

struct DeployInner {
    outcome: Mutex<DeployOutcome>,
    head_block_number: AtomicU64,
    channel_values: Mutex<HashMap<String, ChannelValue>>,
    default_channel_value: Mutex<Option<ChannelValue>>,
    deploy_terms: Mutex<Vec<String>>,
}

/// Handle to the fake validator's behaviour: deploy outcome, head-block index,
/// and per-channel read values.
#[derive(Clone)]
pub struct DeployControl {
    inner: Arc<DeployInner>,
}

impl DeployControl {
    pub(crate) fn start() -> Self {
        Self {
            inner: Arc::new(DeployInner {
                outcome: Mutex::new(DeployOutcome::default()),
                head_block_number: AtomicU64::new(0),
                channel_values: Mutex::new(HashMap::new()),
                default_channel_value: Mutex::new(None),
                deploy_terms: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Configure the outcome the next (and every subsequent) `doDeploy` returns.
    pub fn set_deploy_outcome(&self, outcome: DeployOutcome) {
        *guard(&self.inner.outcome) = outcome;
    }

    /// Configure the head-block index reported by `showMainChain` (depth 1).
    pub fn set_head_block_number(&self, number: u64) {
        self.inner.head_block_number.store(number, Ordering::SeqCst);
    }

    /// Configure the value returned by `getDataAtName` for `channel`.
    pub fn set_channel_value(&self, channel: impl Into<String>, value: ChannelValue) {
        guard(&self.inner.channel_values).insert(channel.into(), value);
    }

    /// Configure the value returned for any channel without a specific entry.
    pub fn set_default_channel_value(&self, value: ChannelValue) {
        *guard(&self.inner.default_channel_value) = Some(value);
    }

    /// The terms of every `doDeploy` the fake has received, in order.
    pub fn recorded_deploy_terms(&self) -> Vec<String> {
        guard(&self.inner.deploy_terms).clone()
    }

    pub(crate) fn deploy_outcome(&self) -> DeployOutcome {
        guard(&self.inner.outcome).clone()
    }

    pub(crate) fn head_block_number(&self) -> u64 {
        self.inner.head_block_number.load(Ordering::SeqCst)
    }

    pub(crate) fn record_deploy_term(&self, term: String) {
        guard(&self.inner.deploy_terms).push(term);
    }

    /// Resolve the `Par` to serve for `channel`, falling back to the default.
    pub(crate) fn channel_par(&self, channel: Option<&str>) -> Option<rhoapi::Par> {
        if let Some(name) = channel {
            let found = guard(&self.inner.channel_values).get(name).cloned();
            if let Some(value) = found {
                return Some(value.into_par());
            }
        }
        guard(&self.inner.default_channel_value)
            .clone()
            .map(ChannelValue::into_par)
    }
}

// ---------------------------------------------------------------------------
// Observer (HTTP explore-deploy) control
// ---------------------------------------------------------------------------

struct ExploreInner {
    responses: Mutex<Vec<Value>>,
    index: AtomicUsize,
    calls: Mutex<Vec<String>>,
    status: Mutex<Value>,
}

/// Handle to the fake observer's `POST /api/explore-deploy` behaviour.
///
/// Responses are served from a scripted list: call `i` returns
/// `responses[min(i, len - 1)]`, so the final scripted response repeats for all
/// later calls. This is what lets a test model the observer visibility lag —
/// script `[empty, empty, populated]` and the retry-on-empty client path is
/// exercised end to end.
#[derive(Clone)]
pub struct ExploreControl {
    inner: Arc<ExploreInner>,
}

impl ExploreControl {
    pub(crate) fn start() -> Self {
        Self {
            inner: Arc::new(ExploreInner {
                responses: Mutex::new(Vec::new()),
                index: AtomicUsize::new(0),
                calls: Mutex::new(Vec::new()),
                status: Mutex::new(json!({ "status": "ok" })),
            }),
        }
    }

    /// Replace the scripted response sequence and reset the call cursor.
    pub fn set_responses(&self, responses: Vec<Value>) {
        *guard(&self.inner.responses) = responses;
        self.inner.index.store(0, Ordering::SeqCst);
    }

    /// Append one response to the scripted sequence.
    pub fn push_response(&self, response: Value) {
        guard(&self.inner.responses).push(response);
    }

    /// Set the JSON body returned by `GET /status`.
    pub fn set_status(&self, status: Value) {
        *guard(&self.inner.status) = status;
    }

    /// The request terms of every explore-deploy call the fake has received.
    pub fn calls(&self) -> Vec<String> {
        guard(&self.inner.calls).clone()
    }

    pub(crate) fn next_response(&self, term: String) -> Value {
        guard(&self.inner.calls).push(term);

        let responses = guard(&self.inner.responses);
        if responses.is_empty() {
            return json!({ "expr": [] });
        }
        let hit = self.inner.index.fetch_add(1, Ordering::SeqCst);
        let index = hit.min(responses.len() - 1);
        responses[index].clone()
    }

    pub(crate) fn status_body(&self) -> Value {
        guard(&self.inner.status).clone()
    }
}

/// Builders for `explore-deploy` response bodies.
///
/// Each returns the `{"expr":[…]}` envelope
/// [`ReadNodeClient`](firefly_client::ReadNodeClient) parses, using the same
/// `ExprString` / `ExprInt` / `ExprBool` tags the client decodes through its
/// `ReadNodeExpr` intermediate model.
pub mod explore {
    use serde_json::{Value, json};

    /// An empty result — the client maps this to `ReturnValueMissing` and, on
    /// the retry path, retries.
    #[must_use]
    pub fn empty() -> Value {
        json!({ "expr": [] })
    }

    /// A string result.
    #[must_use]
    pub fn string(value: &str) -> Value {
        json!({ "expr": [{ "ExprString": { "data": value } }] })
    }

    /// An integer result.
    #[must_use]
    pub fn int(value: i64) -> Value {
        json!({ "expr": [{ "ExprInt": { "data": value } }] })
    }

    /// A boolean result.
    #[must_use]
    pub fn bool(value: bool) -> Value {
        json!({ "expr": [{ "ExprBool": { "data": value } }] })
    }

    /// Wrap an arbitrary `ReadNodeExpr`-shaped object as the single `/expr/0`.
    #[must_use]
    pub fn raw(expr: Value) -> Value {
        let mut object = serde_json::Map::with_capacity(1);
        object.insert("expr".to_owned(), Value::Array(vec![expr]));
        Value::Object(object)
    }
}

// ---------------------------------------------------------------------------
// Observer (WebSocket events) control
// ---------------------------------------------------------------------------

/// Command sent to every connected WebSocket task.
#[derive(Clone)]
pub(crate) enum WsCommand {
    /// Forward this text frame verbatim.
    Send(String),
    /// Close the socket (used to exercise the client's reconnect path).
    Close,
}

struct EventInner {
    tx: broadcast::Sender<WsCommand>,
    connected: AtomicUsize,
    total: AtomicUsize,
    notify: Notify,
}

/// Handle to the fake observer's `/ws/events` stream: inject finalization
/// events and observe / await client connections.
#[derive(Clone)]
pub struct EventControl {
    inner: Arc<EventInner>,
}

/// A deploy to embed in an injected `block-finalised` event.
#[derive(Clone, Debug)]
pub struct InjectedDeploy {
    pub id: String,
    pub cost: u64,
    pub deployer: PublicKey,
    pub errored: bool,
}

impl EventControl {
    pub(crate) fn start() -> Self {
        Self {
            inner: Arc::new(EventInner {
                tx: broadcast::Sender::new(WS_BROADCAST_CAPACITY),
                connected: AtomicUsize::new(0),
                total: AtomicUsize::new(0),
                notify: Notify::new(),
            }),
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<WsCommand> {
        self.inner.tx.subscribe()
    }

    pub(crate) fn on_connect(&self) {
        self.inner.connected.fetch_add(1, Ordering::SeqCst);
        self.inner.total.fetch_add(1, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    pub(crate) fn on_disconnect(&self) {
        self.inner.connected.fetch_sub(1, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    /// Number of WebSocket clients currently connected.
    pub fn connection_count(&self) -> usize {
        self.inner.connected.load(Ordering::SeqCst)
    }

    /// Cumulative number of WebSocket connections since start (never decreases).
    /// Reconnect tests wait on this so a drop-then-reconnect is observable even
    /// though the live count returns to its previous value.
    pub fn total_connections(&self) -> usize {
        self.inner.total.load(Ordering::SeqCst)
    }

    /// Await until at least `n` clients are concurrently connected.
    pub async fn wait_for_connections(&self, n: usize) {
        loop {
            if self.connection_count() >= n {
                return;
            }
            let notified = self.inner.notify.notified();
            if self.connection_count() >= n {
                return;
            }
            notified.await;
        }
    }

    /// Await until the cumulative connection count reaches `n`.
    pub async fn wait_for_total_connections(&self, n: usize) {
        loop {
            if self.total_connections() >= n {
                return;
            }
            let notified = self.inner.notify.notified();
            if self.total_connections() >= n {
                return;
            }
            notified.await;
        }
    }

    /// Broadcast a raw text frame to every connected client. Returns the number
    /// of clients it reached.
    pub fn inject_raw(&self, json: impl Into<String>) -> usize {
        self.inner
            .tx
            .send(WsCommand::Send(json.into()))
            .unwrap_or(0)
    }

    /// Inject a kebab-case `block-finalised` event carrying `deploys`.
    pub fn inject_block_finalised(
        &self,
        block_hash: impl Into<String>,
        deploys: Vec<InjectedDeploy>,
    ) -> usize {
        self.inject_raw(finalised_json(block_hash.into(), deploys))
    }

    /// Inject a `block-added` event (ignored by the client, useful to assert it
    /// does not disturb the deploy path).
    pub fn inject_block_added(
        &self,
        block_hash: impl Into<String>,
        deploys: Vec<InjectedDeploy>,
    ) -> usize {
        let event = WireEvent::BlockAdded {
            payload: WirePayload::new(block_hash.into(), deploys),
        };
        self.inject_raw(to_json(&event))
    }

    /// Inject a `block-created` event (ignored by the client).
    pub fn inject_block_created(
        &self,
        block_hash: impl Into<String>,
        deploys: Vec<InjectedDeploy>,
    ) -> usize {
        let event = WireEvent::BlockCreated {
            payload: WirePayload::new(block_hash.into(), deploys),
        };
        self.inject_raw(to_json(&event))
    }

    /// Close every currently connected socket, forcing reconnect-capable
    /// clients to re-establish their WebSocket.
    pub fn disconnect_all(&self) -> usize {
        self.inner.tx.send(WsCommand::Close).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Wire event serialization
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
enum WireEvent {
    Started,
    BlockAdded { payload: WirePayload },
    BlockCreated { payload: WirePayload },
    BlockFinalised { payload: WirePayload },
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct WirePayload {
    block_hash: String,
    deploys: Vec<WireDeploy>,
}

impl WirePayload {
    fn new(block_hash: String, deploys: Vec<InjectedDeploy>) -> Self {
        Self {
            block_hash,
            deploys: deploys.into_iter().map(WireDeploy::from).collect(),
        }
    }
}

#[derive(Serialize)]
struct WireDeploy {
    id: String,
    cost: u64,
    deployer: PublicKey,
    errored: bool,
}

impl From<InjectedDeploy> for WireDeploy {
    fn from(deploy: InjectedDeploy) -> Self {
        Self {
            id: deploy.id,
            cost: deploy.cost,
            deployer: deploy.deployer,
            errored: deploy.errored,
        }
    }
}

fn to_json(event: &WireEvent) -> String {
    serde_json::to_string(event).expect("wire event is always serializable")
}

fn finalised_json(block_hash: String, deploys: Vec<InjectedDeploy>) -> String {
    to_json(&WireEvent::BlockFinalised {
        payload: WirePayload::new(block_hash, deploys),
    })
}

/// The `started` frame sent to every client on connect.
pub(crate) fn started_json() -> String {
    to_json(&WireEvent::Started)
}
