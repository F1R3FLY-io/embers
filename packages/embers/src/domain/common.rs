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

#[cfg(test)]
mod tests {
    use aes_gcm::Aes256Gcm;
    use aes_gcm::aead::KeyInit;

    use super::*;

    // -------------------------------------------------------
    // prepare_for_signing tests
    // -------------------------------------------------------

    #[test]
    fn test_prepare_for_signing_default_phlo_limit() {
        let contract = prepare_for_signing()
            .code("new Nil".into())
            .valid_after_block_number(0)
            .call();

        let proto = DeployDataProto::decode(contract.0.as_slice())
            .expect("should decode as DeployDataProto");
        assert_eq!(proto.phlo_limit, 5_000_000);
        assert_eq!(proto.term, "new Nil");
        assert_eq!(proto.shard_id, "root");
        assert_eq!(proto.phlo_price, 1);
    }

    #[test]
    fn test_prepare_for_signing_custom_phlo_limit() {
        let contract = prepare_for_signing()
            .code("code".into())
            .valid_after_block_number(10)
            .phlo_limit(PositiveNonZero(1_000_000))
            .call();

        let proto = DeployDataProto::decode(contract.0.as_slice())
            .expect("should decode as DeployDataProto");
        assert_eq!(proto.phlo_limit, 1_000_000);
        assert_eq!(proto.valid_after_block_number, 10);
    }

    #[test]
    fn test_prepare_for_signing_custom_timestamp() {
        let ts = chrono::DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
            .unwrap()
            .to_utc();
        let contract = prepare_for_signing()
            .code("code".into())
            .valid_after_block_number(0)
            .timestamp(ts)
            .call();

        let proto = DeployDataProto::decode(contract.0.as_slice())
            .expect("should decode as DeployDataProto");
        assert_eq!(proto.timestamp, ts.timestamp_millis());
    }

    #[test]
    fn test_prepare_for_signing_shard_id_always_root() {
        let contract = prepare_for_signing()
            .code("test".into())
            .valid_after_block_number(99)
            .call();

        let proto = DeployDataProto::decode(contract.0.as_slice())
            .expect("should decode as DeployDataProto");
        assert_eq!(proto.shard_id, "root");
    }

    // -------------------------------------------------------
    // encrypt/decrypt round-trip tests
    // -------------------------------------------------------

    fn test_aes_key() -> Key<Aes256Gcm> {
        *Key::<Aes256Gcm>::from_slice(&[42u8; 32])
    }

    #[test]
    fn test_encrypt_decrypt_round_trip_string() {
        let key = test_aes_key();
        let original = "hello, world!".to_string();
        let encrypted = serialize_encrypted(&original, &key).expect("should encrypt");
        let decrypted: String = deserialize_decrypted(encrypted, &key).expect("should decrypt");
        assert_eq!(decrypted, original);
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestCredentials {
        email: String,
        token: String,
    }

    #[test]
    fn test_encrypt_decrypt_round_trip_struct() {
        let key = test_aes_key();
        let original = TestCredentials {
            email: "test@example.com".into(),
            token: "secret-token".into(),
        };
        let encrypted = serialize_encrypted(&original, &key).expect("should encrypt");
        let decrypted: TestCredentials =
            deserialize_decrypted(encrypted, &key).expect("should decrypt");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = *Key::<Aes256Gcm>::from_slice(&[1u8; 32]);
        let key2 = *Key::<Aes256Gcm>::from_slice(&[2u8; 32]);
        let encrypted = serialize_encrypted(&"secret", &key1).expect("should encrypt");
        let result = deserialize_decrypted::<String>(encrypted, &key2);
        assert!(result.is_err(), "decryption with wrong key should fail");
    }

    #[test]
    fn test_decrypt_invalid_nonce_fails() {
        let key = test_aes_key();
        let msg = EncryptedMsg {
            nonce: vec![0, 1, 2], // Too short -- AES-256-GCM needs 12 bytes
            ciphertext: vec![0; 32],
        };
        let result = deserialize_decrypted::<String>(msg, &key);
        assert!(result.is_err(), "invalid nonce length should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid nonce length"),
            "expected 'invalid nonce length', got: {err_msg}"
        );
    }

    // -------------------------------------------------------
    // PositiveNonZero tests
    // -------------------------------------------------------

    #[test]
    fn test_positive_non_zero_valid() {
        let result = PositiveNonZero::<i64>::try_from(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 42);
    }

    #[test]
    fn test_positive_non_zero_zero() {
        let result = PositiveNonZero::<i64>::try_from(0);
        assert!(matches!(result, Err(PositiveNonZeroParsingError::Zero)));
    }

    #[test]
    fn test_positive_non_zero_negative() {
        let result = PositiveNonZero::<i64>::try_from(-1);
        assert!(matches!(result, Err(PositiveNonZeroParsingError::Negative)));
    }
}
