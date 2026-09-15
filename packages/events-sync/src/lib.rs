//! # events-sync
//!
//! The reusable core of the `events-sync` CLI: the on-chain contract templates,
//! the [`Entry`] / [`NotifyMsg`] wire types, the two loopback servers (the gRPC
//! communication service the node calls into, and the `ATProto`
//! `com.atproto.sync.subscribeRepos` firehose), the upstream subscribers, and
//! the [`pump_events`] merge loop that assigns each event its monotonically
//! increasing index.
//!
//! The [`events-sync` binary](../main.rs) is a thin CLI over these pieces — the
//! `listen`, `push`, and `init` subcommands are orchestration over the functions
//! exported here. Everything is `pub` so the integration test
//! (`tests/firehose_pipeline.rs`) can stand the same servers up in-process and
//! drive them against a [`firefly-test-node`](firefly_test_node).
//!
//! Nothing here changed behaviourally when it moved out of `main.rs`; the split
//! is a lift-and-shift so the transport, encoding, and ordering logic can be
//! unit-, property-, and pipeline-tested without a live blockchain.

pub mod contracts;

use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Context;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use contracts::{rho_subscribe_to_service, rho_unsubscribe_from_service};
use firefly_client::CommunicationService;
use firefly_client::models::{BlockId, DeployData, DeployId};
use futures::stream::select_all;
use futures::{SinkExt, Stream, StreamExt, TryStreamExt, future};
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::TcpListenerStream;
use tokio_tungstenite::tungstenite::Message;
use tonic::transport::Server;
use uuid::Uuid;
use warp::Filter;

/// One event carried through the firehose: a source payload (`msg`) tagged with
/// the strictly increasing `index` [`pump_events`] assigns it.
///
/// The fields are `pub` so tests and the pipeline can construct and inspect
/// entries; the CLI only ever moves whole `Entry` values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub index: u64,
    pub msg: Vec<u8>,
}

/// The notification a validator's `grpcTell` delivers to the [gRPC communication
/// service](spawn_grpc_server): which deploy, on which channel, holds a fresh
/// batch of events to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyMsg {
    pub deploy_id: DeployId,
    pub channel_name: String,
}

/// Render the Rholang term that stores a batch of `entries` on `channel_name`.
///
/// The batch is `bitcode`-serialized and hex-encoded, then reconstituted on
/// chain with `.hexToBytes()`. Returns the `bitcode` error verbatim so the
/// caller can distinguish an encoding failure from a deploy failure.
pub fn rho_save_events(
    channel_name: impl Display,
    entries: &[Entry],
) -> Result<String, bitcode::Error> {
    bitcode::serialize(&entries).map(|data| {
        let value = format!(
            r#"@"{channel_name}"!("{}".hexToBytes())"#,
            hex::encode(data)
        );
        value
    })
}

/// Render the Rholang term that hands a [`NotifyMsg`] to a service's
/// `notify-listeners` channel, base64-encoding the JSON so it survives as a
/// Rholang string literal.
///
/// # Panics
///
/// Panics if the `NotifyMsg` cannot be serialized to JSON — unreachable for the
/// fixed `{ deploy_id, channel_name }` shape, whose fields are always
/// serializable strings. Preserved verbatim from the original CLI code.
pub fn rho_notify_listeners(service_id: &str, msg: &NotifyMsg) -> String {
    let json_msg = serde_json::to_string(msg).unwrap();
    let json_msg = BASE64_STANDARD.encode(json_msg);
    format!(r#"@"{service_id}-notify-listeners"!("{json_msg}")"#)
}

/// Derive the [`BlockId`] that [`subscribe_to_firefly`] reads a notified
/// deploy's events from.
///
/// KNOWN-DEFECT: this reuses the deploy id string *as if it were a block hash*.
/// With auto-propose enabled on the node a deploy id is no longer a valid block
/// hash — the block that includes the deploy is chosen asynchronously — so the
/// `get_channel_value(block_hash = deploy_id, …)` read this feeds would target a
/// non-existent block on a real node. events-sync needs to source the true block
/// hash from `NodeEvents` (the `block-finalised` event carries it). This
/// function isolates the wrong transform so the defect is observable and pinned
/// by a test (`tests/firehose_pipeline.rs` and the inline unit test) instead of
/// being silently baked into a larger function.
#[must_use]
pub fn notified_block_hash(deploy_id: DeployId) -> BlockId {
    Into::<String>::into(deploy_id).into()
}

/// Spawn the gRPC communication service on a pre-bound loopback `listener`.
///
/// The node calls `send_notification` here; each decoded [`NotifyMsg`] is
/// forwarded onto `events`. The server runs until `shutdown` resolves. Binding
/// the listener beforehand (rather than from a `SocketAddr` inside) lets the
/// caller hand it an ephemeral `127.0.0.1:0` port and learn the address via
/// [`TcpListener::local_addr`] — the CLI binds its configured address, the test
/// binds an ephemeral one.
///
/// # Panics
///
/// The spawned task panics if the notification sink (`events`) is closed when a
/// notification arrives — i.e. the downstream receiver was dropped. Preserved
/// verbatim from the original CLI code (`events.send(...).expect("events
/// closed")`).
pub fn spawn_grpc_server<S>(
    events: mpsc::Sender<NotifyMsg>,
    listener: TcpListener,
    shutdown: S,
) -> tokio::task::JoinHandle<anyhow::Result<()>>
where
    S: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let service = CommunicationService::new(move |msg: NotifyMsg| {
            println!("got grpc notification: {msg:?}");
            let events = events.clone();
            async move {
                events.send(msg).await.expect("events closed");
                Ok(())
            }
        })
        .into_service();

        Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown)
            .await?;

        println!("grpc ended");

        anyhow::Ok(())
    })
}

/// Spawn the `ATProto` `com.atproto.sync.subscribeRepos` firehose on a pre-bound
/// loopback `listener`.
///
/// Each WebSocket subscriber receives a fresh receiver on the `events`
/// broadcast; every [`Entry`] is forwarded as a binary frame carrying its
/// `msg`. The server runs until `shutdown` resolves.
pub fn spawn_atproto_server<S>(
    events: broadcast::Sender<Entry>,
    listener: TcpListener,
    shutdown: S,
) -> tokio::task::JoinHandle<anyhow::Result<()>>
where
    S: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let routes = warp::path!("xrpc" / "com.atproto.sync.subscribeRepos")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                let mut receiver = events.subscribe();

                ws.on_upgrade(move |websocket| async move {
                    let (mut tx, mut rx) = websocket.split();

                    tokio::spawn(async move { while rx.next().await.is_some() {} });

                    while let Ok(entry) = receiver.recv().await {
                        println!("sending ws event");

                        if tx.send(warp::ws::Message::binary(entry.msg)).await.is_err() {
                            return;
                        }
                    }
                })
            });

        warp::serve(routes)
            .incoming(listener)
            .graceful(shutdown)
            .run()
            .await;

        println!("warp ended");

        anyhow::Ok(())
    })
}

/// Connect to an upstream `ATProto` `subscribeRepos` WebSocket and yield each
/// binary frame's bytes, retrying the connection every second until it succeeds.
pub async fn subscribe_to_event_source(
    events_source_url: &str,
) -> Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + Send>> {
    loop {
        let Ok((binary_stream, _)) = tokio_tungstenite::connect_async(events_source_url).await
        else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("failed to connect to ws");
            continue;
        };

        return binary_stream
            .try_filter_map(|event| {
                future::ok(match event {
                    Message::Binary(binary) => Some(binary.to_vec()),
                    _ => None,
                })
            })
            .map_err(Into::into)
            .boxed();
    }
}

/// Subscribe this service to Firefly and yield the events of every deploy the
/// node notifies us about.
///
/// The subscribe contract is deployed up front and an unsubscribe contract is
/// deployed on drop (via `scopeguard`). Each [`NotifyMsg`] arriving on
/// `rx_updates` triggers a channel read at [`notified_block_hash`] — see that
/// function's KNOWN-DEFECT note — whose `bitcode` batch is fanned out one
/// [`Entry::msg`] at a time.
pub async fn subscribe_to_firefly(
    key: &SecretKey,
    mut client: firefly_client::WriteNodeClient,
    service_id: String,
    external_hostname: &str,
    grpc_port: u16,
    mut rx_updates: mpsc::Receiver<NotifyMsg>,
) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + std::marker::Send>>> {
    let self_id = Uuid::new_v4();

    let rho_code = rho_subscribe_to_service(&service_id, self_id, external_hostname, grpc_port);
    let deploy_data = DeployData::builder(rho_code).build();
    client
        .full_deploy(key, deploy_data)
        .await
        .context("failed subscribe to service failed")?;

    let key = *key;
    let mut client = scopeguard::guard(client, move |mut client| {
        tokio::spawn(async move {
            let rho_code = rho_unsubscribe_from_service(&service_id, self_id);
            let deploy_data = DeployData::builder(rho_code).build();
            client
                .full_deploy(&key, deploy_data)
                .await
                .context("failed to unsubscribe from service")?;
            println!("unsubscribed");
            anyhow::Ok(())
        });
    });

    Ok(async_stream::stream! {
        while let Some(event) = rx_updates.recv().await {
            let block_hash: BlockId = notified_block_hash(event.deploy_id);
            let bytes: Vec<u8> = client
                .get_channel_value(block_hash, event.channel_name)
                .await
                .context("failed to get events")?;

            let events: Vec<Entry> =
                bitcode::deserialize(&bytes).context("failed to deserialize events")?;

            println!("got events from firefly: {:?}", events.len());

            for event in events {
                yield Ok(event.msg);
            }
        }
    }
    .boxed())
}

/// Merge any number of extra upstream `subscribeRepos` firehoses into a single
/// stream. An empty `extra_sources` yields an immediately-terminating stream.
pub async fn subscribe_to_atproto_sync(
    extra_sources: Vec<String>,
) -> Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + std::marker::Send>> {
    let streams: Vec<_> = futures::stream::iter(extra_sources)
        .then(|source| async move { subscribe_to_event_source(&source).await })
        .collect()
        .await;

    select_all(streams).boxed()
}

/// Drain `sources`, tag each payload with a strictly increasing `index`, and
/// broadcast it as an [`Entry`] on `tx_events`.
///
/// The loop stops on the first `Err` or on end-of-stream, exactly as the CLI's
/// original inline loop did. A send failure (no live broadcast receivers) is
/// surfaced as an error so the caller decides how to react; the CLI `.expect`s
/// it to preserve its original "panic on closed" behaviour, while tests match
/// on it. `index` is assigned in the order items emerge from the merged stream,
/// so it is globally strictly increasing and per-source order is preserved.
pub async fn pump_events<S>(
    mut sources: S,
    tx_events: broadcast::Sender<Entry>,
) -> anyhow::Result<()>
where
    S: Stream<Item = anyhow::Result<Vec<u8>>> + Unpin,
{
    let mut index = 0;
    while let Some(Ok(msg)) = sources.next().await {
        let self_index = index;
        index += 1;

        tx_events
            .send(Entry {
                msg,
                index: self_index,
            })
            .map_err(|_| anyhow::anyhow!("tx_events closed"))?;
        println!("pushed to atproto");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Entry `bitcode` codec round-trip (the on-chain batch encoding)
    // -----------------------------------------------------------------------

    #[test]
    fn entry_batch_bitcode_roundtrips() {
        let batch = vec![
            Entry {
                index: 0,
                msg: b"first".to_vec(),
            },
            Entry {
                index: 1,
                msg: Vec::new(),
            },
            Entry {
                index: u64::MAX,
                msg: vec![0x00, 0xff, 0x7f, 0x80],
            },
        ];

        let encoded = bitcode::serialize(&batch).expect("batch serializes");
        let decoded: Vec<Entry> = bitcode::deserialize(&encoded).expect("batch deserializes");
        assert_eq!(decoded, batch, "the bitcode codec must round-trip a batch");
    }

    #[test]
    fn empty_entry_batch_bitcode_roundtrips() {
        let batch: Vec<Entry> = Vec::new();
        let encoded = bitcode::serialize(&batch).expect("empty batch serializes");
        let decoded: Vec<Entry> = bitcode::deserialize(&encoded).expect("empty batch deserializes");
        assert!(decoded.is_empty(), "an empty batch stays empty");
    }

    // -----------------------------------------------------------------------
    // rho_save_events — hex field invariant
    // -----------------------------------------------------------------------

    #[test]
    fn rho_save_events_hex_field_decodes_back_to_the_batch() {
        let batch = vec![
            Entry {
                index: 7,
                msg: b"alpha".to_vec(),
            },
            Entry {
                index: 8,
                msg: b"bravo".to_vec(),
            },
        ];
        let rendered = rho_save_events("chan-1", &batch).expect("renders");

        // Exact template shape: @"chan-1"!("<hex>".hexToBytes())
        let prefix = r#"@"chan-1"!(""#;
        let suffix = r#"".hexToBytes())"#;
        assert!(
            rendered.starts_with(prefix),
            "unexpected prefix in: {rendered}"
        );
        assert!(
            rendered.ends_with(suffix),
            "unexpected suffix in: {rendered}"
        );

        let hex_field = &rendered[prefix.len()..rendered.len() - suffix.len()];
        // Invariant: the field is lowercase hex, and decodes to the bitcode of the batch.
        assert!(
            hex_field
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "hex field must be lowercase hex, got: {hex_field}"
        );
        let bytes = hex::decode(hex_field).expect("hex field decodes");
        let decoded: Vec<Entry> = bitcode::deserialize(&bytes).expect("decodes to a batch");
        assert_eq!(decoded, batch);
    }

    // -----------------------------------------------------------------------
    // rho_notify_listeners — base64 field invariant
    // -----------------------------------------------------------------------

    #[test]
    fn rho_notify_listeners_base64_field_decodes_back_to_the_message() {
        let msg = NotifyMsg {
            deploy_id: DeployId::from("deploy-xyz".to_owned()),
            channel_name: "channel-abc".to_owned(),
        };
        let rendered = rho_notify_listeners("svc", &msg);

        let prefix = r#"@"svc-notify-listeners"!(""#;
        let suffix = r#"")"#;
        assert!(
            rendered.starts_with(prefix),
            "unexpected prefix in: {rendered}"
        );
        assert!(
            rendered.ends_with(suffix),
            "unexpected suffix in: {rendered}"
        );

        let b64_field = &rendered[prefix.len()..rendered.len() - suffix.len()];
        let json = BASE64_STANDARD.decode(b64_field).expect("base64 decodes");
        let parsed: NotifyMsg = serde_json::from_slice(&json).expect("json parses");
        assert_eq!(parsed, msg, "the notify payload must round-trip");
    }

    // -----------------------------------------------------------------------
    // KNOWN-DEFECT: deploy id reused as a block hash
    // -----------------------------------------------------------------------

    // KNOWN-DEFECT: `subscribe_to_firefly` reads a notified deploy's events at a
    // block hash it derives by treating the deploy id string AS the block hash.
    // Under auto-propose the deploy id is not a valid block hash (the including
    // block is chosen asynchronously), so on a real node this read targets a
    // non-existent block. This test asserts the current (defective) behaviour —
    // the block id is byte-for-byte the deploy id — so the defect is pinned
    // until events-sync sources the real hash from `NodeEvents`.
    #[test]
    fn notified_block_hash_reuses_the_deploy_id_verbatim() {
        let deploy_id = DeployId::from("deploy-not-a-block-hash".to_owned());
        let block_hash = notified_block_hash(deploy_id.clone());
        assert_eq!(
            block_hash.to_string(),
            "deploy-not-a-block-hash",
            "the defect: the block hash is the deploy id string unchanged"
        );
        assert_eq!(
            String::from(block_hash),
            String::from(deploy_id),
            "block hash and deploy id are the same string"
        );
    }

    // -----------------------------------------------------------------------
    // Property test: pump_events ordering + index monotonicity
    // -----------------------------------------------------------------------

    mod property {
        use proptest::prelude::*;

        use super::*;

        /// The merge loop must (1) assign globally strictly increasing indices
        /// `0, 1, 2, …` in emission order, and (2) preserve each source's
        /// internal order. Each event is tagged `[source_id, position]` so the
        /// per-source subsequence can be recovered from the merged output.
        fn run_case(sources: &[usize]) -> Vec<Entry> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("current-thread runtime builds");

            runtime.block_on(async move {
                let mut streams = Vec::with_capacity(sources.len());
                for (source_id, &count) in sources.iter().enumerate() {
                    let source_id = u8::try_from(source_id).expect("few sources");
                    let tagged: Vec<anyhow::Result<Vec<u8>>> = (0..count)
                        .map(|position| {
                            Ok(vec![
                                source_id,
                                u8::try_from(position).expect("short source"),
                            ])
                        })
                        .collect();
                    streams.push(futures::stream::iter(tagged).boxed());
                }

                let merged = select_all(streams);
                // Capacity strictly exceeds the largest batch these strategies
                // produce (5 sources x 6 events = 30), so no send ever lags.
                let (tx, mut rx) = broadcast::channel::<Entry>(1024);
                pump_events(merged, tx)
                    .await
                    .expect("a live receiver keeps every send successful");

                let mut collected = Vec::new();
                while let Ok(entry) = rx.recv().await {
                    collected.push(entry);
                }
                collected
            })
        }

        proptest! {
            #[test]
            fn indices_strictly_increase_and_per_source_order_is_preserved(
                sources in proptest::collection::vec(0usize..6, 0usize..5),
            ) {
                let collected = run_case(&sources);

                let total: usize = sources.iter().sum();
                prop_assert_eq!(collected.len(), total, "every event is emitted exactly once");

                // (1) indices are exactly 0, 1, …, total - 1 in order.
                for (expected, entry) in collected.iter().enumerate() {
                    prop_assert_eq!(entry.index, expected as u64);
                }

                // (2) per-source order preserved: filtering the merged output by
                // source id yields that source's positions in ascending order.
                for (source_id, &count) in sources.iter().enumerate() {
                    let source_id = u8::try_from(source_id).expect("few sources");
                    let positions: Vec<u8> = collected
                        .iter()
                        .filter(|entry| entry.msg.first() == Some(&source_id))
                        .map(|entry| entry.msg[1])
                        .collect();
                    let expected: Vec<u8> =
                        (0..u8::try_from(count).expect("short source")).collect();
                    prop_assert_eq!(positions, expected, "source {} reordered", source_id);
                }
            }
        }
    }
}
