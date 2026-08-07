//! The `events-sync` binary: a thin CLI over the [`events_sync`] library.
//!
//! The `listen`, `push`, and `init` subcommands are orchestration over the
//! reusable servers, subscribers, contract templates, and the [`pump_events`]
//! merge loop exported by the library crate. See `src/lib.rs` and the package
//! `README.md`.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use events_sync::contracts::rho_init_events_channels;
use events_sync::{
    Entry,
    NotifyMsg,
    pump_events,
    rho_notify_listeners,
    rho_save_events,
    spawn_atproto_server,
    spawn_grpc_server,
    subscribe_to_atproto_sync,
    subscribe_to_event_source,
    subscribe_to_firefly,
};
use firefly_client::models::DeployData;
use futures::stream::select_all;
use futures::{FutureExt, StreamExt, TryStreamExt};
use secp256k1::SecretKey;
use tokio::net::TcpListener;
use tokio::select;
use tokio::signal::ctrl_c;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[derive(Debug, Parser)]
struct Args {
    /// Wallet key in hex format
    #[arg(long)]
    wallet_key: SecretKey,

    /// Firefly deploy service url
    #[arg(long)]
    deploy_service_url: String,

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

    let mut client = firefly_client::WriteNodeClient::new(args.deploy_service_url)
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

            let grpc_listener = TcpListener::bind(communication_service_api_addr)
                .await
                .context("failed to bind communication service listener")?;
            let sync_listener = TcpListener::bind(sync_api_addr)
                .await
                .context("failed to bind atproto sync listener")?;

            let grpc = spawn_grpc_server(tx_updates, grpc_listener, ctrl_c().map(|_| ()));
            let atproto =
                spawn_atproto_server(tx_events.clone(), sync_listener, ctrl_c().map(|_| ()));

            let task = tokio::spawn(async move {
                let all_sources = select_all([
                    subscribe_to_firefly(
                        &args.wallet_key,
                        client,
                        args.service_id,
                        &external_hostname,
                        communication_service_api_addr.port(),
                        rx_updates,
                    )
                    .await?,
                    subscribe_to_atproto_sync(extra_sources).await,
                ]);

                pump_events(all_sources, tx_events)
                    .await
                    .expect("tx_events closed");

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
                    let rho_code = rho_save_events(channel_name, &events)?;
                    let deploy_data = DeployData::builder(rho_code).build();
                    let deploy_id = client
                        .full_deploy(&args.wallet_key, deploy_data)
                        .await
                        .context("failed save events")?;
                    println!("events deployed: {deploy_id}");

                    let rho_code = rho_notify_listeners(
                        &args.service_id,
                        &NotifyMsg {
                            deploy_id,
                            channel_name: channel_name.to_string(),
                        },
                    );
                    let deploy_data = DeployData::builder(rho_code).build();
                    client
                        .full_deploy(&args.wallet_key, deploy_data)
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
            let deploy_data = DeployData::builder(rho_code).build();
            let deploy_id = client
                .full_deploy(&args.wallet_key, deploy_data)
                .await
                .context("failed to init channels")?;
            println!("{deploy_id}");
        }
    }

    Ok(())
}
