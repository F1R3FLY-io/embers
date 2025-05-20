use std::fmt::Display;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Context;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use clap::{Parser, Subcommand};
use futures::stream::select_all;
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryStreamExt, future};
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use tokio::select;
use tokio::signal::ctrl_c;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tonic::transport::Server;
use uuid::Uuid;
use warp::Filter;

#[derive(Debug, Parser)]
struct Args {
    /// Wallet key in hex format
    #[arg(long)]
    wallet_key: SecretKey,

    /// Firefly deploy service url
    #[arg(long)]
    deploy_service_url: String,

    /// Firefly propose service url
    #[arg(long)]
    propose_service_url: String,

    /// Globally unique service identifier
    #[arg(long)]
    service_id: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Listen to updates from firefly
    Listen {
        /// Hostname and port to serve firefly communication service
        #[arg(long)]
        communication_service_api_addr: SocketAddr,

        /// Hostname and port to serve atproto sync api
        #[arg(long)]
        sync_api_addr: SocketAddr,

        /// Hostname this server available externally
        #[arg(long)]
        external_hostname: String,

        /// Extra event sources
        #[arg(long)]
        extra_sources: Vec<String>,
    },

    /// Push updates to firefly
    Push {
        /// Atproto sync url to listen
        #[arg(long)]
        events_source_url: String,

        /// Max window length in seconds
        #[arg(long)]
        time_threshold: u64,

        /// Max number of events in window
        #[arg(long)]
        size_threshold: usize,
    },

    /// Initialize contract
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut client = firefly_client::WriteNodeClient::new(
        args.wallet_key,
        args.deploy_service_url,
        args.propose_service_url,
    )
    .await
    .context("failed to create firefly client")?;

    match args.command {
        Commands::Listen {
            communication_service_api_addr,
            sync_api_addr,
            external_hostname,
            extra_sources,
        } => {
            let (tx_updates, rx_updates) = mpsc::channel::<NotifyMsg>(16);
            let (tx_events, _rx_events) = broadcast::channel::<Entry>(160);

            let grpc = spawn_grpc_server(tx_updates, communication_service_api_addr);
            let atproto = spawn_atproto_server(tx_events.clone(), sync_api_addr);

            let task = tokio::spawn(async move {
                let mut all_sources = select_all([
                    subscribe_to_firefly(
                        client,
                        args.service_id,
                        &external_hostname,
                        communication_service_api_addr.port(),
                        rx_updates,
                    )
                    .await?,
                    subscribe_to_atproto_sync(extra_sources).await,
                ]);

                let mut index = 0;
                while let Some(Ok(msg)) = all_sources.next().await {
                    let self_index = index;
                    index += 1;

                    tx_events
                        .send(Entry {
                            msg,
                            index: self_index,
                        })
                        .expect("tx_events closed");
                    println!("pushed to atproto");
                }

                anyhow::Ok(())
            });

            ctrl_c().await?;
            task.abort();
            let _ = grpc.await;
            let _ = atproto.await;
        }
        Commands::Push {
            events_source_url,
            time_threshold,
            size_threshold,
        } => {
            let mut index = 0;

            let task = async move {
                let binary_stream =
                    subscribe_to_event_source(&events_source_url)
                        .await
                        .map_ok(|msg| {
                            let self_index = index;
                            index += 1;

                            Entry {
                                msg,
                                index: self_index,
                            }
                        });

                let binary_stream = tokio_stream::StreamExt::chunks_timeout(
                    binary_stream,
                    size_threshold,
                    Duration::from_secs(time_threshold),
                );

                tokio::pin!(binary_stream);

                while let Some(events) = binary_stream.next().await {
                    let events = events.into_iter().collect::<Result<Vec<_>, _>>()?;
                    let channel_name = Uuid::new_v4();
                    println!("events: {}", events.len());
                    let rho_code = rho_save_events(channel_name, &events);
                    let hash = client
                        .full_deploy(rho_code)
                        .await
                        .context("failed save events")?;
                    println!("events deployed");

                    let rho_code = rho_notify_listeners(
                        &args.service_id,
                        &NotifyMsg {
                            block_hash: hash,
                            channel_name: channel_name.to_string(),
                        },
                    );
                    client
                        .full_deploy(rho_code)
                        .await
                        .context("failed to notify listeners")?;
                    println!("notified");
                }

                anyhow::Ok(())
            };

            select! {
                _ = task => (),
                _ = ctrl_c() => (),
            };
        }
        Commands::Init => {
            let rho_code = rho_init_events_channels(&args.service_id);
            let hash = client
                .full_deploy(rho_code)
                .await
                .context("failed to init channels")?;
            println!("{hash}");
        }
    }

    Ok(())
}

fn rho_init_events_channels(service_id: &str) -> String {
    format!(
        r#"
        @"{service_id}-listeners"!({{}})|
        contract @"{service_id}-notify-listeners"(@payload) = {{
            new loop, grpcTell(`rho:io:grpcTell`) in {{
                contract loop(@listeners, @payload) = {{
                    match listeners {{
                        [] => Nil
                        [head ...tail] => {{
                            grpcTell!(head.nth(1).get("hostname"), head.nth(1).get("port"), payload)|
                            loop!(tail)
                        }}
                    }}
                }}|
                for(@listeners <<- @"{service_id}-listeners") {{
                    loop!(listeners.toList(), payload)
                }}
            }}
        }}
        "#
    )
}

fn rho_subscribe_to_service(
    service_id: &str,
    self_id: impl Display,
    hostname: &str,
    port: u16,
) -> String {
    format!(
        r#"
        for(@listeners <- @"{service_id}-listeners") {{
            @"{service_id}-listeners"!(listeners.set("{self_id}", {{
                "hostname": "{hostname}",
                "port": {port},
            }}))
        }}
        "#
    )
}

fn rho_unsubscribe_from_service(service_id: &str, self_id: impl Display) -> String {
    format!(
        r#"
        for(@listeners <- @"{service_id}-listeners") {{
            @"{service_id}-listeners"!(listeners.delete("{self_id}"))
        }}
        "#
    )
}

fn rho_save_events(channel_name: impl Display, entries: &[Entry]) -> String {
    let data = bitcode::serialize(&entries).unwrap();
    format!(
        r#"@"{channel_name}"!("{}".hexToBytes())"#,
        hex::encode(data)
    )
}

fn rho_notify_listeners(service_id: &str, msg: &NotifyMsg) -> String {
    let json_msg = serde_json::to_string(msg).unwrap();
    let json_msg = BASE64_STANDARD.encode(json_msg);
    format!(r#"@"{service_id}-notify-listeners"!("{json_msg}")"#)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    index: u64,
    msg: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotifyMsg {
    block_hash: String,
    channel_name: String,
}

fn spawn_atproto_server(
    events: broadcast::Sender<Entry>,
    sync_api_addr: SocketAddr,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
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
            .bind_with_graceful_shutdown(sync_api_addr, ctrl_c().map(|_| ()))
            .1
            .await;

        println!("warp ended");

        anyhow::Ok(())
    })
}

fn spawn_grpc_server(
    events: mpsc::Sender<NotifyMsg>,
    communication_service_api_addr: SocketAddr,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        let service = firefly_client::CommunicationService::new(move |msg: NotifyMsg| {
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
            .serve_with_shutdown(communication_service_api_addr, ctrl_c().map(|_| ()))
            .await?;

        println!("grpc ended");

        anyhow::Ok(())
    })
}

async fn subscribe_to_event_source(
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

async fn subscribe_to_firefly(
    mut client: firefly_client::WriteNodeClient,
    service_id: String,
    external_hostname: &str,
    grpc_port: u16,
    mut rx_updates: mpsc::Receiver<NotifyMsg>,
) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + std::marker::Send>>> {
    let self_id = Uuid::new_v4();

    let rho_code = rho_subscribe_to_service(&service_id, self_id, external_hostname, grpc_port);
    client
        .full_deploy(rho_code)
        .await
        .context("failed to subscribe to service")?;

    let mut client = scopeguard::guard(client, move |mut client| {
        tokio::spawn(async move {
            let rho_code = rho_unsubscribe_from_service(&service_id, self_id);
            client
                .full_deploy(rho_code)
                .await
                .context("failed to unsubscribe from service")?;
            println!("unsubscribed");
            anyhow::Ok(())
        });
    });

    Ok(async_stream::stream! {
        while let Some(event) = rx_updates.recv().await {
            let bytes: Vec<u8> = client
                .get_channel_value(event.block_hash, event.channel_name)
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

async fn subscribe_to_atproto_sync(
    extra_sources: Vec<String>,
) -> Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + std::marker::Send>> {
    let streams: Vec<_> = futures::stream::iter(extra_sources)
        .then(|source| async move { subscribe_to_event_source(&source).await })
        .collect()
        .await;

    select_all(streams).boxed()
}
