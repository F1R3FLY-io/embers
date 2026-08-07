//! # firefly-test-node
//!
//! An in-process, **loopback-only** fake f1r3node — a hermetic L1 test double
//! that the real [`firefly-client`](firefly_client) transport code can be driven
//! against without a running blockchain.
//!
//! It stands up three servers, each bound to `127.0.0.1:0` (an ephemeral port):
//!
//! | Server | Transport | Role | Client that talks to it |
//! | ------ | --------- | ---- | ----------------------- |
//! | validator | gRPC ([`tonic`]) | `casper.v1.DeployService` | [`WriteNodeClient`](firefly_client::WriteNodeClient) |
//! | observer  | HTTP ([`poem`]) | `POST /api/explore-deploy`, `GET /status` | [`ReadNodeClient`](firefly_client::ReadNodeClient) |
//! | observer  | WebSocket ([`poem`]) | `GET /ws/events` | [`NodeEvents`](firefly_client::NodeEvents) |
//!
//! The gRPC service is the *exact* generated service the client imports (the
//! protos are reused from `firefly_client::models`, never a divergent copy), so
//! request/response wire types cannot drift from the client's.
//!
//! ## Usage
//!
//! ```no_run
//! use firefly_test_node::{DeployOutcome, FakeNode};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let node = FakeNode::start().await?;
//! node.deploy().set_head_block_number(7);
//! node.deploy().set_deploy_outcome(DeployOutcome::Accepted("deploy-1".into()));
//!
//! // Point a `firefly_client::WriteNodeClient` at this endpoint to drive the
//! // gRPC path; `node` shuts both loopback servers down when it is dropped.
//! let _validator = node.grpc_endpoint();
//! # Ok(())
//! # }
//! ```
//!
//! ## Fidelity boundary
//!
//! This is a behavioural double, not a node. It does **not**:
//!
//! * evaluate Rholang — `explore-deploy` and channel reads return canned values;
//! * check signatures, phlo/cost, shard, or `valid_after_block_number`;
//! * build a DAG or finalize anything — finalization is whatever a test injects;
//! * persist state or enforce read-after-write ordering beyond the scripted
//!   sequence a test configures.
//!
//! Its purpose is to exercise the client's transport, encoding, retry, and
//! event-dispatch code paths deterministically and offline.

// `pub(crate)` on this crate's cross-module glue (the spawners, `Spawned`, the
// WebSocket command channel) states the intended visibility more clearly than
// the bare `pub` that `redundant_pub_crate` prefers, and keeping the spawners
// `pub(crate)` — they return the crate-internal `Spawned` — avoids the
// `private_interfaces` lint that promoting them to `pub` would trip. This is a
// deliberate, crate-wide visibility policy, not an oversight.
#![allow(clippy::redundant_pub_crate)]

mod control;
mod grpc;
mod observer;

#[cfg(test)]
mod tests;

use std::net::SocketAddr;

pub use control::{
    ChannelValue,
    DeployControl,
    DeployOutcome,
    EventControl,
    ExploreControl,
    InjectedDeploy,
    explore,
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// One spawned loopback server: its bound address, a shutdown trigger, and its
/// task handle. Internal glue shared by the gRPC and observer spawners.
pub(crate) struct Spawned {
    addr: SocketAddr,
    shutdown: oneshot::Sender<()>,
    handle: JoinHandle<()>,
}

/// A running fake node. Holds both servers alive; dropping it shuts them down.
pub struct FakeNode {
    grpc_addr: SocketAddr,
    observer_addr: SocketAddr,
    deploy: DeployControl,
    explore: ExploreControl,
    events: EventControl,
    shutdown: Vec<oneshot::Sender<()>>,
    tasks: Vec<JoinHandle<()>>,
}

impl FakeNode {
    /// Start a fake node with default behaviour on ephemeral loopback ports.
    pub async fn start() -> anyhow::Result<Self> {
        Self::builder().start().await
    }

    /// Begin configuring a fake node before it starts.
    #[must_use]
    pub fn builder() -> FakeNodeBuilder {
        FakeNodeBuilder::default()
    }

    /// The validator gRPC socket address (always loopback).
    #[must_use]
    pub const fn grpc_addr(&self) -> SocketAddr {
        self.grpc_addr
    }

    /// The observer socket address serving HTTP and WebSocket (always loopback).
    #[must_use]
    pub const fn observer_addr(&self) -> SocketAddr {
        self.observer_addr
    }

    /// `http://<grpc_addr>` — pass to [`WriteNodeClient::new`](firefly_client::WriteNodeClient::new).
    #[must_use]
    pub fn grpc_endpoint(&self) -> String {
        format!("http://{}", self.grpc_addr)
    }

    /// `http://<observer_addr>` — pass to [`ReadNodeClient::new`](firefly_client::ReadNodeClient::new).
    #[must_use]
    pub fn observer_http_url(&self) -> String {
        format!("http://{}", self.observer_addr)
    }

    /// `ws://<observer_addr>` — pass to [`NodeEvents::new`](firefly_client::NodeEvents::new)
    /// (which appends `/ws/events`).
    #[must_use]
    pub fn observer_ws_url(&self) -> String {
        format!("ws://{}", self.observer_addr)
    }

    /// Handle to the validator's deploy / head-block / channel-read behaviour.
    #[must_use]
    pub const fn deploy(&self) -> &DeployControl {
        &self.deploy
    }

    /// Handle to the observer's `explore-deploy` scripting.
    #[must_use]
    pub const fn explore(&self) -> &ExploreControl {
        &self.explore
    }

    /// Handle to the observer's WebSocket event injection and connection counts.
    #[must_use]
    pub const fn events(&self) -> &EventControl {
        &self.events
    }

    /// Gracefully stop both servers and await their tasks.
    pub async fn shutdown(mut self) {
        for trigger in self.shutdown.drain(..) {
            let _ = trigger.send(());
        }
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }
}

impl Drop for FakeNode {
    fn drop(&mut self) {
        // Dropping the senders resolves each server's shutdown future; aborting
        // the tasks is the hard backstop that guarantees the ephemeral ports are
        // released even if a server is mid-request when the handle goes away.
        self.shutdown.clear();
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Builder for [`FakeNode`], for setting initial behaviour before the servers
/// start. Everything it configures can also be changed at runtime through the
/// control handles once the node is running.
#[derive(Default)]
pub struct FakeNodeBuilder {
    deploy_outcome: Option<DeployOutcome>,
    head_block_number: Option<u64>,
    explore_responses: Option<Vec<serde_json::Value>>,
}

impl FakeNodeBuilder {
    /// Set the initial `doDeploy` outcome.
    #[must_use]
    pub fn deploy_outcome(mut self, outcome: DeployOutcome) -> Self {
        self.deploy_outcome = Some(outcome);
        self
    }

    /// Set the initial head-block index.
    #[must_use]
    pub const fn head_block_number(mut self, number: u64) -> Self {
        self.head_block_number = Some(number);
        self
    }

    /// Set the initial scripted `explore-deploy` response sequence.
    #[must_use]
    pub fn explore_responses(mut self, responses: Vec<serde_json::Value>) -> Self {
        self.explore_responses = Some(responses);
        self
    }

    /// Bind the loopback listeners and start both servers.
    pub async fn start(self) -> anyhow::Result<FakeNode> {
        let deploy = DeployControl::start();
        if let Some(outcome) = self.deploy_outcome {
            deploy.set_deploy_outcome(outcome);
        }
        if let Some(number) = self.head_block_number {
            deploy.set_head_block_number(number);
        }

        let explore = ExploreControl::start();
        if let Some(responses) = self.explore_responses {
            explore.set_responses(responses);
        }

        let events = EventControl::start();

        let grpc = grpc::spawn(deploy.clone()).await?;
        let observer = observer::spawn(explore.clone(), events.clone()).await?;

        Ok(FakeNode {
            grpc_addr: grpc.addr,
            observer_addr: observer.addr,
            deploy,
            explore,
            events,
            shutdown: vec![grpc.shutdown, observer.shutdown],
            tasks: vec![grpc.handle, observer.handle],
        })
    }
}
