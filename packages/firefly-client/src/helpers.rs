use std::collections::HashMap;

use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Utc};
use prost::Message as _;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};

use crate::models::casper::DeployDataProto;
use crate::models::rhoapi::expr::ExprInstance;
use crate::models::{SignedCode, rhoapi};

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

/// Signs the exact protobuf bytes returned by a prepare endpoint.
///
/// Keeping the prepared bytes and signature together allows callers to persist
/// a deploy in an outbox and safely resubmit the identical deploy after a crash.
pub fn sign_prepared_contract(contract: Vec<u8>, key: &SecretKey) -> anyhow::Result<SignedCode> {
    let prepared = DeployDataProto::decode(contract.as_slice())?;
    anyhow::ensure!(
        prepared.sig.is_empty(),
        "prepared deploy already contains a signature"
    );
    anyhow::ensure!(
        prepared.deployer.is_empty(),
        "prepared deploy already contains a deployer"
    );
    anyhow::ensure!(
        prepared.sig_algorithm.is_empty(),
        "prepared deploy already contains a signature algorithm"
    );

    let digest = Blake2b::<U32>::new().chain_update(&contract).finalize();
    let secp = Secp256k1::new();
    let signature = secp.sign_ecdsa(Message::from_digest(digest.into()), key);
    Ok(SignedCode {
        contract,
        sig: signature.serialize_der().to_vec(),
        sig_algorithm: "secp256k1".to_owned(),
        deployer: key.public_key(&secp).serialize_uncompressed().to_vec(),
    })
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

#[test]
fn prepared_contract_signing_is_stable_and_rejects_signed_input() {
    let key = SecretKey::from_byte_array([7u8; 32]).expect("valid key");
    let prepared = DeployDataProto {
        term: "new Nil".to_owned(),
        timestamp: 42,
        shard_id: "root".to_owned(),
        ..Default::default()
    }
    .encode_to_vec();
    let first = sign_prepared_contract(prepared.clone(), &key).expect("signing should succeed");
    let second = sign_prepared_contract(prepared, &key).expect("signing should be deterministic");
    assert_eq!(first.contract, second.contract);
    assert_eq!(first.sig, second.sig);
    assert_eq!(first.deployer, second.deployer);

    let mut already_signed =
        DeployDataProto::decode(first.contract.as_slice()).expect("prepared protobuf");
    already_signed.sig = vec![1].into();
    assert!(sign_prepared_contract(already_signed.encode_to_vec(), &key).is_err());
}

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use secp256k1::ecdsa::Signature;

    use super::*;

    /// Any 32-byte value in the valid secp256k1 range.
    fn arb_secret_key() -> impl Strategy<Value = SecretKey> {
        any::<[u8; 32]>()
            .prop_filter_map("valid secp256k1 secret key", |bytes| SecretKey::from_byte_array(bytes).ok())
    }

    proptest! {
        /// A signed deploy is self-consistent for every key and term: the DER
        /// signature it carries verifies against the deployer public key it
        /// records, over the blake2b-256 digest of the prepared bytes.
        #[test]
        fn signed_contract_verifies(key in arb_secret_key(), term in any::<String>()) {
            let bytes = DeployDataProto { term, ..Default::default() }.encode_to_vec();
            let signed = sign_prepared_contract(bytes.clone(), &key)
                .expect("a freshly prepared contract signs");

            let secp = Secp256k1::new();
            let digest = Blake2b::<U32>::new().chain_update(&bytes).finalize();
            let message = Message::from_digest(digest.into());
            let signature = Signature::from_der(&signed.sig).expect("the DER signature parses");
            let deployer = PublicKey::from_slice(&signed.deployer).expect("the deployer key parses");
            prop_assert!(secp.verify_ecdsa(message, &signature, &deployer).is_ok());
        }

        /// Signing is deterministic (RFC 6979) for every key and term, so an
        /// outboxed deploy can be resubmitted byte-identically after a crash.
        #[test]
        fn signing_is_deterministic(key in arb_secret_key(), term in any::<String>()) {
            let bytes = DeployDataProto { term, ..Default::default() }.encode_to_vec();
            let first = sign_prepared_contract(bytes.clone(), &key).expect("signs");
            let second = sign_prepared_contract(bytes, &key).expect("signs");
            prop_assert_eq!(first.sig, second.sig);
        }
    }
}
