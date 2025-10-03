use anyhow::Context;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Utc};
use firefly_client::WriteNodeClient;
use firefly_client::models::{BlockId, DeployData, rhoapi};
use firefly_client::rendering::{Render, Uri};
use prost::Message as _;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha3::digest::consts::U32;

use crate::ai_agents::models::InitAgentsEnv;
use crate::ai_agents_teams::models::InitAgentsTeamsEnv;
use crate::wallets::models::InitWalletsEnv;

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_mainnet_contracts(
    client: &mut WriteNodeClient,
    key: &SecretKey,
) -> anyhow::Result<BlockId> {
    let code = InitAgentsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy agents env")?;

    let code = InitAgentsTeamsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy agents teams env")?;

    let code = InitWalletsEnv.render()?;
    let deploy_data = DeployData::builder(code).build();
    client
        .deploy(key, deploy_data)
        .await
        .context("failed to deploy wallets env")?;

    client.propose().await
}

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/init.rho")]
struct InitTestnetEnv {
    env_uri: Uri,
    nonce: i64,
    public_key: Vec<u8>,
    sig: Vec<u8>,
}

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn bootstrap_testnet_contracts(
    client: &mut WriteNodeClient,
    deployer_key: &SecretKey,
    env_key: &SecretKey,
) -> anyhow::Result<BlockId> {
    let secp = Secp256k1::new();
    let env_public_key = PublicKey::from_secret_key(&secp, env_key);
    let deployer_public_key = PublicKey::from_secret_key(&secp, deployer_key);

    let timestamp = chrono::Utc::now();
    let nonce = 0;
    let sig = insert_signed_signature(env_key, timestamp, &deployer_public_key, nonce);

    let code = InitTestnetEnv {
        env_uri: env_public_key.into(),
        nonce,
        public_key: env_public_key.serialize_uncompressed().into(),
        sig,
    }
    .render()?;
    let deploy_data = DeployData::builder(code).timestamp(timestamp).build();

    client
        .deploy(deployer_key, deploy_data)
        .await
        .context("failed to testnet env")?;

    client.propose().await
}

fn insert_signed_signature(
    key: &SecretKey,
    timestamp: DateTime<Utc>,
    deployer: &PublicKey,
    nonce: i64,
) -> Vec<u8> {
    let par = rhoapi::Par {
        exprs: vec![rhoapi::Expr {
            expr_instance: Some(rhoapi::expr::ExprInstance::ETupleBody(rhoapi::ETuple {
                ps: vec![
                    rhoapi::Par {
                        exprs: vec![rhoapi::Expr {
                            expr_instance: Some(rhoapi::expr::ExprInstance::GInt(
                                timestamp.timestamp_millis(),
                            )),
                        }],
                        ..Default::default()
                    },
                    rhoapi::Par {
                        exprs: vec![rhoapi::Expr {
                            expr_instance: Some(rhoapi::expr::ExprInstance::GByteArray(
                                deployer.serialize_uncompressed().into(),
                            )),
                        }],
                        ..Default::default()
                    },
                    rhoapi::Par {
                        exprs: vec![rhoapi::Expr {
                            expr_instance: Some(rhoapi::expr::ExprInstance::GInt(nonce)),
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            })),
        }],
        ..Default::default()
    }
    .encode_to_vec();

    let hash = Blake2b::<U32>::new().chain_update(par).finalize();

    Secp256k1::new()
        .sign_ecdsa(Message::from_digest(hash.into()), key)
        .serialize_der()
        .to_vec()
}

#[test]
fn test_insert_signed_signature() {
    use std::str::FromStr;

    let secp = Secp256k1::new();
    let timestamp = DateTime::from_timestamp_millis(1559156356769).unwrap();
    let secret_key =
        SecretKey::from_str("f450b26bac63e5dd9343cd46f5fae1986d367a893cd21eedd98a4cb3ac699abc")
            .unwrap();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let nonce = 9223372036854775807;

    let sig = insert_signed_signature(&secret_key, timestamp, &public_key, nonce);

    assert_eq!(
        hex::encode(sig),
        "3044022038044777f2faccfc503363ce70d5701ae64969ca98e64049f92d8477fdea0c1402200843c073c6f0121f580f38bb2940f16cef54fc24ea325ebc00230fa6e3117549"
    );
}
