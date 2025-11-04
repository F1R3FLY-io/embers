use std::collections::HashMap;

use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Utc};
use prost::Message as _;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};

use crate::models::rhoapi;
use crate::models::rhoapi::expr::ExprInstance;

pub trait FromExpr: Sized {
    fn from(val: ExprInstance) -> anyhow::Result<Self>;
}

impl FromExpr for String {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::GString(value) => Ok(value),
            other => Err(anyhow!("unexpected expr type: {other:?} expected GString")),
        }
    }
}

impl<T> FromExpr for Vec<T>
where
    T: FromExpr,
{
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::EListBody(list) => list
                .ps
                .into_iter()
                .map(|par| {
                    let expr = par.exprs.into_iter().next().context("missing exprs")?;
                    let expr = expr.expr_instance.context("missing expr_instance")?;
                    T::from(expr)
                })
                .collect(),
            other => Err(anyhow!(
                "unexpected expr type: {other:?} expected EListBody"
            )),
        }
    }
}

impl<T> FromExpr for HashMap<String, T>
where
    T: FromExpr,
{
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::EMapBody(map) => map
                .kvs
                .into_iter()
                .map(|pair| {
                    let key = pair
                        .key
                        .and_then(|key| key.exprs.into_iter().next())
                        .and_then(|expr| expr.expr_instance)
                        .context("missing key")?;
                    let key = FromExpr::from(key)?;

                    let value = pair
                        .value
                        .and_then(|value| value.exprs.into_iter().next())
                        .and_then(|expr| expr.expr_instance)
                        .context("missing value")?;
                    let value = FromExpr::from(value)?;

                    Ok((key, value))
                })
                .collect(),
            other => Err(anyhow!("unexpected expr type: {other:?} expected EMapBody")),
        }
    }
}

impl FromExpr for Vec<u8> {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::GByteArray(list) => Ok(list),
            other => Err(anyhow!(
                "unexpected expr type: {other:?} expected GByteArray"
            )),
        }
    }
}

pub trait ShortHex {
    fn short_hex(&self, length: usize) -> String;
}

impl<T> ShortHex for T
where
    T: AsRef<[u8]>,
{
    fn short_hex(&self, length: usize) -> String {
        let slice = self.as_ref();
        if slice.len() > length {
            format!("{}...", hex::encode(&slice[..length]))
        } else {
            hex::encode(slice)
        }
    }
}

pub fn insert_signed_signature(
    key: &SecretKey,
    timestamp: DateTime<Utc>,
    deployer: &PublicKey,
    version: i64,
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
                            expr_instance: Some(rhoapi::expr::ExprInstance::GInt(version)),
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
    let timestamp = DateTime::from_timestamp_millis(1_559_156_356_769).unwrap();
    let secret_key =
        SecretKey::from_str("f450b26bac63e5dd9343cd46f5fae1986d367a893cd21eedd98a4cb3ac699abc")
            .unwrap();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let version = 9_223_372_036_854_775_807;

    let sig = insert_signed_signature(&secret_key, timestamp, &public_key, version);

    assert_eq!(
        hex::encode(sig),
        "3044022038044777f2faccfc503363ce70d5701ae64969ca98e64049f92d8477fdea0c1402200843c073c6f0121f580f38bb2940f16cef54fc24ea325ebc00230fa6e3117549"
    );
}
