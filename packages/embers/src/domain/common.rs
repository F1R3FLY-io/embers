use aes_gcm::aead::{Aead, AeadCore, KeyInit, Nonce, OsRng};
use aes_gcm::{Aes256Gcm, Key};
use atrium_api::agent::Agent;
use atrium_api::types::BlobRef;
use chrono::{DateTime, Utc};
use firefly_client::helpers::ShortHex;
use firefly_client::models::casper::DeployDataProto;
use prost::Message;
use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};

use crate::domain::agents_teams::models::EncryptedMsg;

macro_rules! record_trace {
    ($($value:ident),+ $(,)?) => {
        if ::tracing::enabled!(::tracing::Level::TRACE) {
            let span = ::tracing::Span::current();
            $(
                span.record(stringify!($value), ::tracing::field::debug(&$value));
            )+
        }
    };
}

pub(crate) use record_trace;

#[bon::builder]
pub fn prepare_for_signing(
    code: String,
    valid_after_block_number: u64,
    phlo_limit: Option<PositiveNonZero<i64>>,
    timestamp: Option<DateTime<Utc>>,
) -> PreparedContract {
    let timestamp = timestamp
        .unwrap_or_else(chrono::Utc::now)
        .timestamp_millis();
    let contract = DeployDataProto {
        term: code,
        timestamp,
        phlo_price: 1,
        phlo_limit: phlo_limit.map_or(5_000_000, |v| v.0),
        valid_after_block_number: valid_after_block_number as _,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract(contract)
}

pub fn serialize_encrypted<T>(val: T, key: &Key<Aes256Gcm>) -> anyhow::Result<EncryptedMsg>
where
    T: Serialize,
{
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let data = serde_json::to_string(&val)?;
    let ciphertext = cipher.encrypt(&nonce, data.as_ref())?;

    Ok(EncryptedMsg {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

#[allow(unused)]
pub fn deserialize_decrypted<T>(
    EncryptedMsg { ciphertext, nonce }: EncryptedMsg,
    key: &Key<Aes256Gcm>,
) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let cipher = Aes256Gcm::new(key);
    #[allow(deprecated)]
    let nonce = Nonce::<Aes256Gcm>::from_exact_iter(nonce)
        .ok_or_else(|| anyhow::anyhow!("invalid nonce length"))?;
    let data = cipher.decrypt(&nonce, ciphertext.as_ref())?;
    serde_json::from_slice(&data).map_err(Into::into)
}

pub async fn upload_blob_from_url<S>(agent: &Agent<S>, url: &str) -> anyhow::Result<BlobRef>
where
    S: atrium_api::agent::SessionManager + Send + Sync,
{
    let resp = reqwest::get(url).await?;
    let bytes = resp.bytes().await?;
    let blob_ref = agent
        .api
        .com
        .atproto
        .repo
        .upload_blob(bytes.to_vec())
        .await?;

    Ok(blob_ref.data.blob)
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PositiveNonZero<T>(pub T);

#[derive(Debug, Clone, thiserror::Error)]
pub enum PositiveNonZeroParsingError {
    #[error("value is zero")]
    Zero,
    #[error("value is negative")]
    Negative,
}

impl TryFrom<i64> for PositiveNonZero<i64> {
    type Error = PositiveNonZeroParsingError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(Self::Error::Zero);
        }

        if value < 0 {
            return Err(Self::Error::Negative);
        }

        Ok(Self(value))
    }
}

#[derive(derive_more::Debug, Clone)]
#[debug("{:?}", _0.short_hex(32))]
pub struct PreparedContract(pub Vec<u8>);

#[derive(Debug, Clone)]
pub struct RegistryDeploy {
    pub timestamp: DateTime<Utc>,
    pub version: i64,
    pub uri_pub_key: PublicKey,
    pub signature: Vec<u8>,
}
