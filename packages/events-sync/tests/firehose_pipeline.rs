//! In-process, loopback-only end-to-end test of the events-sync `listen`
//! pipeline.
//!
//! It stands up the same three moving parts the CLI's `listen` subcommand does —
//! the gRPC communication service ([`spawn_grpc_server`]), the `ATProto`
//! `com.atproto.sync.subscribeRepos` firehose ([`spawn_atproto_server`]), and a
//! [`firefly-test-node`](firefly_test_node) fake validator the
//! [`WriteNodeClient`](firefly_client::WriteNodeClient) reads canned batches
//! from — wires them together with [`pump_events`], and drives one firehose
//! notification all the way through:
//!
//! ```text
//!   gRPC send_notification  ──▶ mpsc ──▶ subscribe_to_firefly
//!         (ExternalCommunicationServiceClient)        │  get_channel_value
//!                                                      ▼  (fake node: canned batch)
//!   ws://…/xrpc/com.atproto.sync.subscribeRepos ◀── pump_events (broadcast Entry)
//! ```
//!
//! Every socket is `127.0.0.1:0` (ephemeral, race-free: the listeners are bound
//! first and their addresses read back), so the whole test is hermetic L1.

use std::time::Duration;

use anyhow::Context as _;
use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use events_sync::{
    Entry,
    NotifyMsg,
    pump_events,
    spawn_atproto_server,
    spawn_grpc_server,
    subscribe_to_atproto_sync,
    subscribe_to_firefly,
};
use firefly_client::WriteNodeClient;
use firefly_client::models::DeployId;
use firefly_client::models::casper::UpdateNotification;
use firefly_client::models::casper::v1::external_communication_service_client::ExternalCommunicationServiceClient;
use firefly_test_node::{ChannelValue, FakeNode};
use futures::StreamExt as _;
use futures::stream::select_all;
use secp256k1::SecretKey;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

/// The on-chain channel the notified batch lives on.
const CHANNEL_NAME: &str = "events-sync-pipeline-channel";
/// The deploy id the notification carries — deliberately *not* a real block
/// hash, so the KNOWN-DEFECT below is observable.
const DEPLOY_ID: &str = "deploy-777-not-a-block-hash";

/// A deterministic loopback wallet key (the fake validator never checks it).
fn test_key() -> SecretKey {
    let mut bytes = [0u8; 32];
    bytes[31] = 1;
    SecretKey::from_byte_array(bytes).expect("valid secret key")
}

/// What one end-to-end run observed: the binary frames that reached the `ATProto`
/// subscriber, and the block hashes the fake validator was asked to read at.
struct PipelineRun {
    received_frames: Vec<Vec<u8>>,
    recorded_read_block_hashes: Vec<String>,
}

/// Drive the full `listen` pipeline once with `frames` as the canned firehose
/// batch, delivering a single notification and reading `frames.len()` frames
/// back off the `ATProto` WebSocket.
async fn run_pipeline(frames: &[Vec<u8>]) -> anyhow::Result<PipelineRun> {
    // 1. Fake validator, pre-loaded with the bitcode batch the notified read
    //    will return. Each canned Entry carries a source index that pump_events
    //    will overwrite with its own — the ATProto server forwards only `msg`.
    let node = FakeNode::start().await.context("start fake node")?;
    let canned: Vec<Entry> = frames
        .iter()
        .enumerate()
        .map(|(index, msg)| Entry {
            index: index as u64,
            msg: msg.clone(),
        })
        .collect();
    let canned_bytes = bitcode::serialize(&canned).context("serialize canned batch")?;
    node.deploy()
        .set_channel_value(CHANNEL_NAME, ChannelValue::Bytes(canned_bytes));

    let client = WriteNodeClient::new(node.grpc_endpoint())
        .await
        .context("connect write client to fake node")?;

    // 2. The two loopback servers, on race-free ephemeral ports.
    let grpc_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind grpc comm listener")?;
    let grpc_addr = grpc_listener.local_addr().context("grpc addr")?;
    let sync_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind atproto sync listener")?;
    let sync_addr = sync_listener.local_addr().context("sync addr")?;

    let (tx_updates, rx_updates) = mpsc::channel::<NotifyMsg>(16);
    let (tx_events, _keepalive_rx) = broadcast::channel::<Entry>(160);

    let grpc_server = spawn_grpc_server(tx_updates, grpc_listener, std::future::pending());
    let atproto_server =
        spawn_atproto_server(tx_events.clone(), sync_listener, std::future::pending());

    // 3. The firefly source + the pump loop, exactly as `listen` composes them.
    let key = test_key();
    let firefly = subscribe_to_firefly(
        &key,
        client,
        "pipeline-service".to_owned(),
        "127.0.0.1",
        grpc_addr.port(),
        rx_updates,
    )
    .await
    .context("subscribe to firefly")?;

    let pump = tokio::spawn(async move {
        let sources = select_all([firefly, subscribe_to_atproto_sync(Vec::new()).await]);
        pump_events(sources, tx_events).await
    });

    // 4. An ATProto consumer subscribes BEFORE any event flows, so its broadcast
    //    receiver exists when the pump fans the batch out.
    let ws_url = format!("ws://{sync_addr}/xrpc/com.atproto.sync.subscribeRepos");
    let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .context("connect atproto subscriber")?;
    let (_ws_write, mut ws_read) = ws_stream.split();

    // 5. The node notifies events-sync of a fresh batch, via the same gRPC call
    //    a validator's `grpcTell` makes: a base64 JSON NotifyMsg.
    let notify = NotifyMsg {
        deploy_id: DeployId::from(DEPLOY_ID.to_owned()),
        channel_name: CHANNEL_NAME.to_owned(),
    };
    let payload = BASE64_STANDARD.encode(serde_json::to_string(&notify).context("encode notify")?);
    let mut comm = ExternalCommunicationServiceClient::connect(format!("http://{grpc_addr}"))
        .await
        .context("connect comm client")?;
    comm.send_notification(tonic::Request::new(UpdateNotification {
        client_host: "127.0.0.1".to_owned(),
        client_port: 0,
        payload,
    }))
    .await
    .context("send notification")?;

    // 6. Read exactly the expected number of binary frames off the firehose.
    let mut received_frames = Vec::with_capacity(frames.len());
    while received_frames.len() < frames.len() {
        let message = tokio::time::timeout(Duration::from_secs(10), ws_read.next())
            .await
            .context("timed out waiting for a firehose frame")?
            .context("firehose closed before all frames arrived")?
            .context("firehose read error")?;
        match message {
            Message::Binary(bytes) => received_frames.push(bytes.to_vec()),
            Message::Close(_) => break,
            // Ignore control frames (ping/pong) and any text frames.
            _ => {}
        }
    }

    // Capture what the fake validator was asked to read before teardown.
    let recorded_read_block_hashes = node.deploy().recorded_read_block_hashes();

    // 7. Teardown on the LIVE runtime: aborting and awaiting the pump drops the
    //    firefly stream (and its scopeguard unsubscribe `tokio::spawn`) while the
    //    runtime still accepts spawns, avoiding a spawn-during-shutdown panic.
    pump.abort();
    let _ = pump.await;
    grpc_server.abort();
    atproto_server.abort();
    node.shutdown().await;

    Ok(PipelineRun {
        received_frames,
        recorded_read_block_hashes,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn firehose_frames_arrive_in_order() {
    let frames: Vec<Vec<u8>> = vec![
        b"repo-frame-0".to_vec(),
        b"repo-frame-1".to_vec(),
        b"repo-frame-2".to_vec(),
        b"repo-frame-3".to_vec(),
    ];

    let run = run_pipeline(&frames)
        .await
        .expect("pipeline runs end to end");

    assert_eq!(
        run.received_frames, frames,
        "the ATProto subscriber must receive every firehose frame, in the batch's order"
    );
}

// KNOWN-DEFECT: events-sync reads a notified deploy's events at a block hash it
// derives by treating the deploy id string AS the block hash (see
// `events_sync::notified_block_hash` / `subscribe_to_firefly`). Under the node's
// auto-propose, a deploy id is not a valid block hash — the including block is
// chosen asynchronously — so on a real node this read targets a non-existent
// block and returns nothing. Here the fake validator answers regardless of block
// hash, which lets us pin the defect at the wire: the block hash it was asked to
// read at is byte-for-byte the deploy id from the notification. This asserts the
// CURRENT (wrong) behaviour; it must change to sourcing the real hash from
// NodeEvents, at which point this test should be updated to expect the true hash.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deploy_id_is_reused_as_block_hash() {
    let frames: Vec<Vec<u8>> = vec![b"only-frame".to_vec()];

    let run = run_pipeline(&frames)
        .await
        .expect("pipeline runs end to end");

    assert_eq!(
        run.recorded_read_block_hashes,
        vec![DEPLOY_ID.to_owned()],
        "the channel read must have used the deploy id verbatim as the block hash"
    );
}
