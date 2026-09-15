use std::collections::HashMap;
use std::fmt::Display;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Ok, anyhow};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use clap::{Parser, Subcommand};
use firefly_client::helpers::FromExpr;
use firefly_client::models::rhoapi::expr::ExprInstance;
use firefly_client::models::{BlockId, DeployData, DeployId};
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
        hash: BlockId,
    },

    /// Initialize contract
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut client = firefly_client::WriteNodeClient::new(args.deploy_service_url).await?;

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
                let deploy_data = DeployData::builder(rho_code).build();
                let deploy_id = client.full_deploy(&args.wallet_key, deploy_data).await?;
                println!("dump deploy_id: {deploy_id}");

                let rho_code = rho_save_hash_template(
                    &args.service_id,
                    &ServiceHash {
                        deploy_id,
                        channel_name,
                    },
                );
                let deploy_data = DeployData::builder(rho_code).build();
                let deploy_id = client.full_deploy(&args.wallet_key, deploy_data).await?;
                println!("save deploy_id: {deploy_id}");
            }
        }
        Commands::Download { hash } => {
            let entries: Vec<ServiceHash> = client
                .get_channel_value(hash.clone(), format!("{}-hashes", args.service_id))
                .await?;

            let Some(entry) = entries.into_iter().next_back() else {
                return Err(anyhow!("no data"));
            };

            let sql: String = client
                .get_channel_value(hash, entry.channel_name.to_string())
                .await?;
            let sql = BASE64_STANDARD.decode(sql)?;
            let sql = String::from_utf8(sql)?;
            println!("{sql}");
        }
        Commands::Init => {
            let rho_code = rho_save_hash_contract(&args.service_id);
            let deploy_data = DeployData::builder(rho_code).build();
            let deploy_id = client.full_deploy(&args.wallet_key, deploy_data).await?;
            println!("{deploy_id}");
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ServiceHash {
    deploy_id: DeployId,
    channel_name: Uuid,
}

impl FromExpr for ServiceHash {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        let mut map: HashMap<String, String> = FromExpr::from(val)?;

        let deploy_id = map.remove("deploy_id").context("deploy_id is missing")?;
        let channel_name = map
            .remove("channel_name")
            .context("channel_name is missing")?;
        let channel_name: Uuid = channel_name.parse()?;

        Ok(Self {
            deploy_id: deploy_id.into(),
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

#[cfg(test)]
mod tests {
    use firefly_client::models::rhoapi;

    use super::*;

    /// A single-`GString` `Par`, the shape a channel-name key/value takes.
    fn string_par(value: &str) -> rhoapi::Par {
        rhoapi::Par {
            exprs: vec![rhoapi::Expr {
                expr_instance: Some(ExprInstance::GString(value.to_owned())),
            }],
            ..Default::default()
        }
    }

    /// Build the `EMapBody` a read node returns for a Rholang map of strings —
    /// the exact expression [`ServiceHash`]'s [`FromExpr`] decodes.
    fn string_map_expr(pairs: &[(&str, &str)]) -> ExprInstance {
        ExprInstance::EMapBody(rhoapi::EMap {
            kvs: pairs
                .iter()
                .map(|(key, value)| rhoapi::KeyValuePair {
                    key: Some(string_par(key)),
                    value: Some(string_par(value)),
                })
                .collect(),
            ..Default::default()
        })
    }

    /// Extract the base64 payload from a `@"chan"!("<base64>")` dump template.
    /// The base64 alphabet excludes `!`, `(`, `"`, and `)`, so the delimiters are
    /// unambiguous even for an arbitrary decoded dump.
    fn dump_template_base64(rendered: &str) -> &str {
        let open = rendered
            .find("!(\"")
            .expect("dump template has a payload open")
            + 3;
        let close = rendered
            .rfind("\")")
            .expect("dump template has a payload close");
        &rendered[open..close]
    }

    // -----------------------------------------------------------------------
    // Exact-string assertions on the rendered contract templates
    // -----------------------------------------------------------------------

    #[test]
    fn rho_sql_dump_template_is_exact() {
        // base64("hi") == "aGk="
        let rendered = rho_sql_dump_template("dump-chan", "hi".to_owned());
        assert_eq!(rendered, r#"@"dump-chan"!("aGk=")"#);
    }

    #[test]
    fn rho_save_hash_template_is_exact() {
        let service_hash = ServiceHash {
            deploy_id: DeployId::from("dep-1".to_owned()),
            channel_name: Uuid::nil(),
        };
        let rendered = rho_save_hash_template("svc", &service_hash);
        assert_eq!(
            rendered,
            r#"@"svc-hash"!({"deploy_id":"dep-1","channel_name":"00000000-0000-0000-0000-000000000000"})"#
        );
    }

    #[test]
    fn rho_save_hash_contract_is_exact() {
        let rendered = rho_save_hash_contract("svc");
        let expected = concat!(
            "\n",
            "        @\"svc-hashes\"!([])\n",
            "        |\n",
            "        contract @\"svc-hash\"(@data) = {\n",
            "            for(@hashes <- @\"svc-hashes\") {\n",
            "                @\"svc-hashes\"!(hashes ++ [data])\n",
            "            }\n",
            "        }\n",
            "        ",
        );
        assert_eq!(rendered, expected);
    }

    // -----------------------------------------------------------------------
    // ServiceHash FromExpr round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn service_hash_from_expr_roundtrips() {
        let channel_name = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let original = ServiceHash {
            deploy_id: DeployId::from("dep-42".to_owned()),
            channel_name,
        };

        // The read node hands back a Rholang map of strings; FromExpr must
        // reconstruct the ServiceHash from it (and parse the UUID).
        let expr = string_map_expr(&[
            ("deploy_id", "dep-42"),
            ("channel_name", &channel_name.to_string()),
        ]);
        let parsed = <ServiceHash as FromExpr>::from(expr).expect("ServiceHash parses");
        assert_eq!(
            parsed, original,
            "ServiceHash must round-trip through FromExpr"
        );
    }

    #[test]
    fn service_hash_from_expr_missing_channel_name_errors() {
        let expr = string_map_expr(&[("deploy_id", "dep-1")]);
        let error = <ServiceHash as FromExpr>::from(expr)
            .expect_err("a map without channel_name must not parse");
        assert!(
            error.to_string().contains("channel_name is missing"),
            "unexpected error: {error}"
        );
    }

    // -----------------------------------------------------------------------
    // Property: base64 encoding of an arbitrary dump round-trips
    // -----------------------------------------------------------------------

    mod property {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            /// For any SQL dump, the base64 the dump template embeds decodes back
            /// to the exact original bytes — the invariant `download` relies on.
            #[test]
            fn base64_dump_roundtrips(sql in any::<String>()) {
                let rendered = rho_sql_dump_template(Uuid::nil(), sql.clone());
                let field = dump_template_base64(&rendered);
                let decoded = BASE64_STANDARD.decode(field).expect("base64 decodes");
                let restored = String::from_utf8(decoded).expect("decoded bytes are utf8");
                prop_assert_eq!(restored, sql);
            }
        }
    }

    // -----------------------------------------------------------------------
    // L2 integration: real pg_dump round-trip (never runs in L1)
    // -----------------------------------------------------------------------

    /// Exercises the actual `pg_dump` invocation and the base64 dump-template
    /// round-trip against a live `PostgreSQL` — the full `upload`/`download` data
    /// path minus the chain. Ignored so L1 never shells out to `pg_dump`; run it
    /// explicitly (`cargo test -p state-sync -- --ignored`) with a reachable
    /// database. Reads `STATE_SYNC_TEST_DB_URL`, defaulting to a local trust
    /// server on 5432.
    #[test]
    #[ignore = "L2: requires local postgres on 5432"]
    fn pg_dump_roundtrip_against_local_postgres() {
        let db_url = std::env::var("STATE_SYNC_TEST_DB_URL")
            .unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5432/postgres".to_owned());

        let sql = run_pg_dump(&db_url).expect("pg_dump should succeed against a local postgres");

        // The dump must survive the base64 template round-trip byte-for-byte.
        let rendered = rho_sql_dump_template(Uuid::nil(), sql.clone());
        let field = dump_template_base64(&rendered);
        let decoded = BASE64_STANDARD.decode(field).expect("base64 decodes");
        let restored = String::from_utf8(decoded).expect("decoded bytes are utf8");
        assert_eq!(
            restored, sql,
            "the pg_dump output must round-trip through base64"
        );
    }
}
