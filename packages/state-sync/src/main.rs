use std::collections::HashMap;
use std::fmt::Display;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Ok, anyhow};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use clap::{Parser, Subcommand};
use firefly_client::{helpers::FromExpr, models::rhoapi::expr::ExprInstance};
use secp256k1::SecretKey;
use serde::{Deserialize, Serialize};
use tokio::select;
use uuid::Uuid;

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
    /// Upload db snaphot
    Upload {
        /// Postgres connection string
        #[arg(long)]
        db_url: String,

        /// Sync interval in seconds
        #[arg(long)]
        interval: u64,
    },

    /// Download db snaphot
    Download {
        /// Block hash
        #[arg(long)]
        hash: String,
    },

    /// Initialize contract
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut client =
        firefly_client::WriteNodeClient::new(args.deploy_service_url, args.propose_service_url)
            .await?;

    match args.command {
        Commands::Upload { db_url, interval } => {
            let mut interval = tokio::time::interval(Duration::from_secs(interval));

            let mut exit = tokio::spawn(tokio::signal::ctrl_c());

            loop {
                select! {
                    _ = interval.tick() => (),
                    _ = &mut exit => break,
                };

                let channel_name = Uuid::new_v4();
                let sql = run_pg_dump(&db_url)?;

                let rho_code = rho_sql_dump_template(channel_name, sql);
                let hash = client
                    .full_deploy(&args.wallet_key, rho_code.into())
                    .await?;
                println!("dump hash: {hash}");

                let rho_code = rho_save_hash_template(
                    &args.service_id,
                    &ServiceHash {
                        block_hash: hash,
                        channel_name,
                    },
                );
                let hash = client
                    .full_deploy(&args.wallet_key, rho_code.into())
                    .await?;
                println!("save hash: {hash}");
            }
        }
        Commands::Download { hash } => {
            let entries: Vec<ServiceHash> = client
                .get_channel_value(hash, format!("{}-hashes", args.service_id))
                .await?;

            let Some(entry) = entries.into_iter().next_back() else {
                return Err(anyhow!("no data"));
            };

            let sql: String = client
                .get_channel_value(entry.block_hash, entry.channel_name.to_string())
                .await?;
            let sql = BASE64_STANDARD.decode(sql)?;
            let sql = String::from_utf8(sql)?;
            println!("{sql}");
        }
        Commands::Init => {
            let rho_code = rho_save_hash_contract(&args.service_id);
            let hash = client
                .full_deploy(&args.wallet_key, rho_code.into())
                .await?;
            println!("{hash}");
        }
    }

    Ok(())
}

fn rho_sql_dump_template(channel_name: impl Display, sql: String) -> String {
    format!(r#"@"{channel_name}"!("{}")"#, BASE64_STANDARD.encode(sql))
}

fn rho_save_hash_template(service_id: impl Display, service_hash: &ServiceHash) -> String {
    format!(
        r#"@"{service_id}-hash"!({})"#,
        serde_json::to_string(service_hash).unwrap()
    )
}

fn rho_save_hash_contract(service_id: &str) -> String {
    format!(
        r#"
        @"{service_id}-hashes"!([])
        |
        contract @"{service_id}-hash"(@data) = {{
            for(@hashes <- @"{service_id}-hashes") {{
                @"{service_id}-hashes"!(hashes ++ [data])
            }}
        }}
        "#
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceHash {
    block_hash: String,
    channel_name: Uuid,
}

impl FromExpr for ServiceHash {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        let mut map: HashMap<String, String> = FromExpr::from(val)?;

        let block_hash = map.remove("block_hash").context("block_hash is missing")?;
        let channel_name = map
            .remove("channel_name")
            .context("channel_name is missing")?;
        let channel_name: Uuid = channel_name.parse()?;

        Ok(Self {
            block_hash,
            channel_name,
        })
    }
}

fn run_pg_dump(db_url: &str) -> anyhow::Result<String> {
    let mut command = Command::new("pg_dump");

    command.arg("--data-only");
    command.arg("--column-inserts");
    command.arg("--exclude-schema=public");
    command.arg(db_url);

    let output = command.output()?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(Into::into)
    } else {
        Err(anyhow::anyhow!(
            "error from pg_dump: {:?}",
            String::from_utf8(output.stderr)
        ))
    }
}
