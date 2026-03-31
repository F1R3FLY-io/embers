use std::collections::HashMap;
use std::marker::PhantomData;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Utc};
use crc::Crc;
use derive_more::{AsRef, Display, From, Into};
use digest::OutputSizeUser;
use digest::typenum::Unsigned;
pub use f1r3node_models::{casper, rhoapi, servicemodelapi};
use secp256k1::PublicKey;
use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::helpers::ShortHex;
use crate::rendering::{IntoValue, Value};

#[derive(
    Debug,
    Clone,
    Display,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    AsRef,
    Into,
    From,
    Serialize,
    Deserialize,
)]
pub struct BlockId(String);

impl IntoValue for BlockId {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[derive(
    Debug,
    Clone,
    Display,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    AsRef,
    Into,
    From,
    Serialize,
    Deserialize,
)]
pub struct DeployId(String);

impl IntoValue for DeployId {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[derive(derive_more::Debug, Clone)]
pub struct SignedCode {
    #[debug("{:?}", contract.short_hex(32))]
    pub contract: Vec<u8>,
    #[debug("{:?}", hex::encode(sig))]
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    #[debug("{:?}", hex::encode(deployer))]
    pub deployer: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ReadNodeExprUnforg {
    UnforgPrivate { data: String },
    UnforgDeploy { data: String },
    UnforgDeployer { data: String },
}

impl From<ReadNodeExprUnforg> for serde_json::Value {
    fn from(value: ReadNodeExprUnforg) -> Self {
        match value {
            ReadNodeExprUnforg::UnforgPrivate { data } => Self::String(data),
            ReadNodeExprUnforg::UnforgDeploy { data } => Self::String(data),
            ReadNodeExprUnforg::UnforgDeployer { data } => Self::String(data),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum ReadNodeExpr {
    ExprTuple { data: Vec<Self> },
    ExprList { data: Vec<Self> },
    ExprSet { data: Vec<Self> },
    ExprMap { data: HashMap<String, Self> },

    ExprNil {},
    ExprBool { data: bool },
    ExprInt { data: serde_json::Number },
    ExprString { data: String },
    ExprBytes { data: String },
    ExprUri { data: String },
    ExprUnforg { data: ReadNodeExprUnforg },
}

impl From<ReadNodeExpr> for serde_json::Value {
    fn from(value: ReadNodeExpr) -> Self {
        match value {
            ReadNodeExpr::ExprTuple { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprList { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprSet { data } => {
                Self::Array(data.into_iter().map(Into::into).collect())
            }
            ReadNodeExpr::ExprMap { data } => {
                Self::Object(data.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
            ReadNodeExpr::ExprNil {} => Self::Null,
            ReadNodeExpr::ExprBool { data } => Self::Bool(data),
            ReadNodeExpr::ExprInt { data } => Self::Number(data),
            ReadNodeExpr::ExprString { data } => Self::String(data),
            ReadNodeExpr::ExprBytes { data } => Self::String(data),
            ReadNodeExpr::ExprUri { data } => Self::String(data),
            ReadNodeExpr::ExprUnforg { data } => data.into(),
        }
    }
}

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn to_result(self) -> Result<R, L> {
        self.into()
    }
}

impl<L, R> From<Either<L, R>> for Result<R, L> {
    fn from(value: Either<L, R>) -> Self {
        match value {
            Either::Left(err) => Err(err),
            Either::Right(v) => Ok(v),
        }
    }
}

impl<'de, L, R> Deserialize<'de> for Either<L, R>
where
    L: Deserialize<'de>,
    R: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_tuple(
            2,
            EitherVisitor {
                phantom: PhantomData,
            },
        )
    }
}

struct EitherVisitor<L, R> {
    phantom: PhantomData<(L, R)>,
}

impl<'de, L, R> de::Visitor<'de> for EitherVisitor<L, R>
where
    L: Deserialize<'de>,
    R: Deserialize<'de>,
{
    type Value = Either<L, R>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence of (bool, value)")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let valid: bool = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;

        if valid {
            seq.next_element()?.map(Either::Right)
        } else {
            seq.next_element()?.map(Either::Left)
        }
        .ok_or_else(|| de::Error::invalid_length(1, &self))
    }
}

#[derive(Debug, Clone)]
pub enum ValidAfter {
    Head,
    Index(u64),
}

#[derive(Debug, Clone, bon::Builder)]
pub struct DeployData {
    #[builder(start_fn)]
    pub term: String,

    #[builder(default = 5_000_000)]
    pub phlo_limit: u64,

    #[builder(default = chrono::Utc::now())]
    pub timestamp: DateTime<Utc>,

    #[builder(default = ValidAfter::Head)]
    pub valid_after_block_number: ValidAfter,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum NodeEvent {
    Started,
    BlockAdded { payload: BlockEventPayload },
    BlockCreated { payload: BlockEventPayload },
    BlockFinalised { payload: BlockEventPayload },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockEventPayload {
    pub block_hash: BlockId,
    pub deploys: Vec<BlockEventDeploy>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockEventDeploy {
    pub id: DeployId,
    pub cost: u64,
    pub deployer: PublicKey,
    pub errored: bool,
}

pub const FIRECAP_ID: [u8; 3] = [0, 0, 0];
pub const FIRECAP_VERSION: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Into, AsRef)]
pub struct WalletAddress(String);

impl IntoValue for WalletAddress {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[derive(Debug, Clone, Error)]
pub enum ParseWalletAddressError {
    #[error("internal encoder error: {0}")]
    EncoderError(bs58::decode::Error),

    #[error("invalid address size: {0}")]
    InvalidRevAddressSize(usize),

    #[error("invalid address format: {0}")]
    InvalidAddress(String),
}

impl TryFrom<String> for WalletAddress {
    type Error = ParseWalletAddressError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let decoded = bs58::decode(&value)
            .into_vec()
            .map_err(Self::Error::EncoderError)?;

        let (payload, checksum) = decoded
            .split_at_checked(decoded.len().wrapping_sub(4))
            .ok_or(ParseWalletAddressError::InvalidRevAddressSize(
                decoded.len(),
            ))?;

        let hash = Blake2b::<U32>::new().chain_update(payload).finalize();

        if checksum != &hash[..4] {
            return Err(ParseWalletAddressError::InvalidAddress(value));
        }

        Ok(Self(value))
    }
}

impl From<PublicKey> for WalletAddress {
    fn from(key: PublicKey) -> Self {
        let key_hash: [u8; 32] = sha3::Keccak256::new()
            .chain_update(&key.serialize_uncompressed()[1..])
            .finalize()
            .into();

        let eth_hash = sha3::Keccak256::new()
            .chain_update(&key_hash[key_hash.len() - 20..])
            .finalize();

        let checksum_hash: [u8; 32] = Blake2b::<U32>::new()
            .chain_update(FIRECAP_ID)
            .chain_update([FIRECAP_VERSION])
            .chain_update(eth_hash)
            .finalize()
            .into();

        let checksum = &checksum_hash[0..4];

        let address_bytes = [
            FIRECAP_ID.as_ref(),
            [FIRECAP_VERSION].as_ref(),
            eth_hash.as_ref(),
            checksum,
        ]
        .concat();

        Self(bs58::encode(address_bytes).into_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Into, AsRef)]
pub struct Uri(String);

const CRC14: crc::Algorithm<u16> = crc::Algorithm {
    width: 14,
    poly: 0x4805,
    init: 0x0000,
    refin: false,
    refout: false,
    xorout: 0x0000,
    check: 0,
    residue: 0x0000,
};

impl From<PublicKey> for Uri {
    fn from(value: PublicKey) -> Self {
        let hash = Blake2b::<U32>::new()
            .chain_update(value.serialize_uncompressed())
            .finalize();

        let crc = Crc::<u16>::new(&CRC14).checksum(&hash).to_ne_bytes();
        let full_key = [hash.as_ref(), [crc[0], crc[1] << 2].as_ref()].concat();
        let encoded = zbase32::encode(&full_key, 270);
        Self(format!("rho:id:{encoded}"))
    }
}

#[derive(Debug, Clone, Error)]
pub enum ParseUriError {
    #[error("invalid uri prefix")]
    IvalidPrefix,

    #[error("invalid zbase32: {0}")]
    InvalidZBase32(&'static str),

    #[error("invalid decoded bytes length")]
    InvalidDecodedLength,

    #[error("checksum mistmatch")]
    ChecksumMistmatch,
}

impl TryFrom<String> for Uri {
    type Error = ParseUriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const HASH_SIZE: usize = <Blake2b<U32> as OutputSizeUser>::OutputSize::USIZE;

        let encoded = value
            .strip_prefix("rho:id:")
            .ok_or(Self::Error::IvalidPrefix)?;
        let decoded = zbase32::decode_str(encoded, 270).map_err(Self::Error::InvalidZBase32)?;
        let bytes: [u8; HASH_SIZE + 2] = decoded
            .try_into()
            .map_err(|_| Self::Error::InvalidDecodedLength)?;

        let (hash, crc_bytes) = bytes.split_at(HASH_SIZE);
        let crc = u16::from_ne_bytes([crc_bytes[0], crc_bytes[1] >> 2]);
        let expected = Crc::<u16>::new(&CRC14).checksum(hash);

        if expected != crc {
            return Err(Self::Error::ChecksumMistmatch);
        }

        Ok(Self(value))
    }
}

impl IntoValue for Uri {
    fn into_value(self) -> Value {
        Value::Uri(self.0)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Helper: deterministic key pair for tests.
    fn test_keypair() -> (secp256k1::SecretKey, secp256k1::PublicKey) {
        let secp = secp256k1::Secp256k1::new();
        let secret_key =
            secp256k1::SecretKey::from_byte_array([1u8; 32]).expect("valid 32-byte secret key");
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        (secret_key, public_key)
    }

    // -----------------------------------------------------------------------
    // WalletAddress
    // -----------------------------------------------------------------------

    #[test]
    fn wallet_address_from_public_key_roundtrips() {
        let (_sk, pk) = test_keypair();
        let addr = WalletAddress::from(pk);
        let addr_string: String = addr.clone().into();

        // Re-parsing the produced string must succeed and yield the same value.
        let parsed = WalletAddress::try_from(addr_string.clone())
            .expect("WalletAddress produced by From<PublicKey> must parse successfully");
        assert_eq!(
            addr, parsed,
            "roundtripped WalletAddress should equal the original"
        );
    }

    #[test]
    fn wallet_address_try_from_invalid_base58() {
        // '0', 'O', 'I', 'l' are not in the Base58 alphabet.
        let result = WalletAddress::try_from("0OIl!!!".to_string());
        assert!(
            result.is_err(),
            "invalid base58 characters should produce an error"
        );
        assert!(
            matches!(
                result.unwrap_err(),
                ParseWalletAddressError::EncoderError(_)
            ),
            "expected EncoderError variant for bad base58"
        );
    }

    #[test]
    fn wallet_address_try_from_too_short() {
        // A valid base58 string that decodes to fewer than 5 bytes (payload needs
        // at least 1 byte + 4 checksum bytes).
        let result = WalletAddress::try_from("1".to_string());
        assert!(
            result.is_err(),
            "a too-short address should produce an error"
        );
        assert!(
            matches!(
                result.unwrap_err(),
                ParseWalletAddressError::InvalidRevAddressSize(_)
            ),
            "expected InvalidRevAddressSize for too-short input"
        );
    }

    #[test]
    fn wallet_address_try_from_invalid_checksum() {
        let (_sk, pk) = test_keypair();
        let addr = WalletAddress::from(pk);
        let addr_string: String = addr.into();

        // Corrupt the last character of the base58 string to invalidate the checksum.
        let mut corrupted = addr_string.clone();
        let last = corrupted.pop().expect("non-empty address");
        // Pick a different valid base58 character.
        let replacement = if last == '1' { '2' } else { '1' };
        corrupted.push(replacement);

        let result = WalletAddress::try_from(corrupted);
        assert!(
            result.is_err(),
            "a corrupted checksum should produce an error"
        );
        assert!(
            matches!(
                result.unwrap_err(),
                ParseWalletAddressError::InvalidAddress(_)
            ),
            "expected InvalidAddress for bad checksum"
        );
    }

    // -----------------------------------------------------------------------
    // Uri
    // -----------------------------------------------------------------------

    #[test]
    fn uri_from_public_key_roundtrips() {
        let (_sk, pk) = test_keypair();
        let uri = Uri::from(pk);
        let uri_string: String = uri.clone().into();

        assert!(
            uri_string.starts_with("rho:id:"),
            "URI must start with rho:id: prefix, got: {uri_string}"
        );

        let parsed = Uri::try_from(uri_string.clone())
            .expect("Uri produced by From<PublicKey> must parse successfully");
        assert_eq!(uri, parsed, "roundtripped Uri should equal the original");
    }

    #[test]
    fn uri_try_from_invalid_prefix() {
        let result = Uri::try_from("invalid:prefix:abc".to_string());
        assert!(result.is_err(), "wrong prefix should produce an error");
        assert!(
            matches!(result.unwrap_err(), ParseUriError::IvalidPrefix),
            "expected IvalidPrefix variant"
        );
    }

    #[test]
    fn uri_try_from_invalid_zbase32() {
        // zbase32 alphabet is ybndrfg8ejkmcpqxot1uwisza345h769.
        // 'v' and '2' are NOT in the alphabet. We need a 54-char string
        // (270 bits / 5 bits per char) so the library does not panic on
        // length before checking character validity.
        let bad_encoded = "v".repeat(54);
        let result = Uri::try_from(format!("rho:id:{bad_encoded}"));
        assert!(
            result.is_err(),
            "invalid zbase32 characters should produce an error"
        );
        assert!(
            matches!(result.unwrap_err(), ParseUriError::InvalidZBase32(_)),
            "expected InvalidZBase32 variant"
        );
    }

    #[test]
    fn uri_try_from_invalid_checksum() {
        let (_sk, pk) = test_keypair();
        let uri = Uri::from(pk);
        let uri_string: String = uri.into();

        // Flip a character in the encoded portion to corrupt the checksum.
        let encoded_part = uri_string
            .strip_prefix("rho:id:")
            .expect("has rho:id: prefix");
        let mut chars: Vec<char> = encoded_part.chars().collect();
        // Change the first character to something different but still valid zbase32.
        chars[0] = if chars[0] == 'y' { 'b' } else { 'y' };
        let corrupted = format!("rho:id:{}", chars.into_iter().collect::<String>());

        let result = Uri::try_from(corrupted);
        assert!(
            result.is_err(),
            "corrupted checksum should produce an error"
        );
        // May be ChecksumMistmatch or InvalidDecodedLength depending on what
        // the corruption does to the decoded bytes.
        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                ParseUriError::ChecksumMistmatch | ParseUriError::InvalidDecodedLength
            ),
            "expected ChecksumMistmatch or InvalidDecodedLength, got: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // ReadNodeExpr -> serde_json::Value  conversion
    // -----------------------------------------------------------------------

    #[test]
    fn read_node_expr_nil_converts_to_null() {
        let expr = ReadNodeExpr::ExprNil {};
        let val: serde_json::Value = expr.into();
        assert_eq!(val, serde_json::Value::Null, "ExprNil should become null");
    }

    #[test]
    fn read_node_expr_bool_converts() {
        let expr_true = ReadNodeExpr::ExprBool { data: true };
        let expr_false = ReadNodeExpr::ExprBool { data: false };
        assert_eq!(
            serde_json::Value::from(expr_true),
            json!(true),
            "ExprBool(true)"
        );
        assert_eq!(
            serde_json::Value::from(expr_false),
            json!(false),
            "ExprBool(false)"
        );
    }

    #[test]
    fn read_node_expr_int_converts() {
        let expr = ReadNodeExpr::ExprInt {
            data: serde_json::Number::from(42),
        };
        assert_eq!(serde_json::Value::from(expr), json!(42), "ExprInt(42)");
    }

    #[test]
    fn read_node_expr_string_converts() {
        let expr = ReadNodeExpr::ExprString {
            data: "hello".to_string(),
        };
        assert_eq!(serde_json::Value::from(expr), json!("hello"), "ExprString");
    }

    #[test]
    fn read_node_expr_bytes_converts() {
        let expr = ReadNodeExpr::ExprBytes {
            data: "deadbeef".to_string(),
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!("deadbeef"),
            "ExprBytes"
        );
    }

    #[test]
    fn read_node_expr_uri_converts() {
        let expr = ReadNodeExpr::ExprUri {
            data: "rho:id:abc".to_string(),
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!("rho:id:abc"),
            "ExprUri"
        );
    }

    #[test]
    fn read_node_expr_tuple_converts() {
        let expr = ReadNodeExpr::ExprTuple {
            data: vec![
                ReadNodeExpr::ExprInt {
                    data: serde_json::Number::from(1),
                },
                ReadNodeExpr::ExprBool { data: true },
            ],
        };
        assert_eq!(serde_json::Value::from(expr), json!([1, true]), "ExprTuple");
    }

    #[test]
    fn read_node_expr_list_converts() {
        let expr = ReadNodeExpr::ExprList {
            data: vec![
                ReadNodeExpr::ExprString {
                    data: "a".to_string(),
                },
                ReadNodeExpr::ExprString {
                    data: "b".to_string(),
                },
            ],
        };
        assert_eq!(serde_json::Value::from(expr), json!(["a", "b"]), "ExprList");
    }

    #[test]
    fn read_node_expr_set_converts() {
        let expr = ReadNodeExpr::ExprSet {
            data: vec![ReadNodeExpr::ExprInt {
                data: serde_json::Number::from(99),
            }],
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!([99]),
            "ExprSet should become a JSON array"
        );
    }

    #[test]
    fn read_node_expr_map_converts() {
        let mut map = HashMap::new();
        map.insert(
            "key".to_string(),
            ReadNodeExpr::ExprString {
                data: "value".to_string(),
            },
        );
        let expr = ReadNodeExpr::ExprMap { data: map };
        assert_eq!(
            serde_json::Value::from(expr),
            json!({"key": "value"}),
            "ExprMap"
        );
    }

    #[test]
    fn read_node_expr_unforg_private_converts() {
        let expr = ReadNodeExpr::ExprUnforg {
            data: ReadNodeExprUnforg::UnforgPrivate {
                data: "priv123".to_string(),
            },
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!("priv123"),
            "ExprUnforg(Private)"
        );
    }

    #[test]
    fn read_node_expr_unforg_deploy_converts() {
        let expr = ReadNodeExpr::ExprUnforg {
            data: ReadNodeExprUnforg::UnforgDeploy {
                data: "deploy456".to_string(),
            },
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!("deploy456"),
            "ExprUnforg(Deploy)"
        );
    }

    #[test]
    fn read_node_expr_unforg_deployer_converts() {
        let expr = ReadNodeExpr::ExprUnforg {
            data: ReadNodeExprUnforg::UnforgDeployer {
                data: "deployer789".to_string(),
            },
        };
        assert_eq!(
            serde_json::Value::from(expr),
            json!("deployer789"),
            "ExprUnforg(Deployer)"
        );
    }

    // -----------------------------------------------------------------------
    // ReadNodeExpr deserialization from JSON
    // -----------------------------------------------------------------------

    #[test]
    fn deserialize_expr_nil() {
        let v = json!({"ExprNil": {}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprNil should deserialize");
        assert!(
            matches!(expr, ReadNodeExpr::ExprNil {}),
            "expected ExprNil variant"
        );
    }

    #[test]
    fn deserialize_expr_bool() {
        let v = json!({"ExprBool": {"data": true}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprBool should deserialize");
        assert!(
            matches!(expr, ReadNodeExpr::ExprBool { data: true }),
            "expected ExprBool {{ data: true }}"
        );
    }

    #[test]
    fn deserialize_expr_int() {
        let v = json!({"ExprInt": {"data": -7}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprInt should deserialize");
        match expr {
            ReadNodeExpr::ExprInt { data } => {
                assert_eq!(data, serde_json::Number::from(-7), "expected -7");
            }
            other => panic!("expected ExprInt, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_string() {
        let v = json!({"ExprString": {"data": "hello world"}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprString should deserialize");
        match expr {
            ReadNodeExpr::ExprString { data } => {
                assert_eq!(data, "hello world");
            }
            other => panic!("expected ExprString, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_bytes() {
        let v = json!({"ExprBytes": {"data": "cafebabe"}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprBytes should deserialize");
        match expr {
            ReadNodeExpr::ExprBytes { data } => {
                assert_eq!(data, "cafebabe");
            }
            other => panic!("expected ExprBytes, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_uri() {
        let v = json!({"ExprUri": {"data": "rho:id:xyz"}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprUri should deserialize");
        match expr {
            ReadNodeExpr::ExprUri { data } => {
                assert_eq!(data, "rho:id:xyz");
            }
            other => panic!("expected ExprUri, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_tuple() {
        let v = json!({"ExprTuple": {"data": [{"ExprInt": {"data": 1}}, {"ExprBool": {"data": false}}]}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprTuple should deserialize");
        match expr {
            ReadNodeExpr::ExprTuple { data } => {
                assert_eq!(data.len(), 2, "tuple should have 2 elements");
            }
            other => panic!("expected ExprTuple, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_list() {
        let v = json!({"ExprList": {"data": [{"ExprString": {"data": "a"}}]}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprList should deserialize");
        match expr {
            ReadNodeExpr::ExprList { data } => {
                assert_eq!(data.len(), 1, "list should have 1 element");
            }
            other => panic!("expected ExprList, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_set() {
        let v = json!({"ExprSet": {"data": []}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprSet should deserialize");
        match expr {
            ReadNodeExpr::ExprSet { data } => {
                assert!(data.is_empty(), "set should be empty");
            }
            other => panic!("expected ExprSet, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_map() {
        let v = json!({"ExprMap": {"data": {"k": {"ExprNil": {}}}}});
        let expr: ReadNodeExpr = serde_json::from_value(v).expect("ExprMap should deserialize");
        match expr {
            ReadNodeExpr::ExprMap { data } => {
                assert!(data.contains_key("k"), "map should contain key 'k'");
            }
            other => panic!("expected ExprMap, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_unforg_private() {
        let v = json!({"ExprUnforg": {"data": {"UnforgPrivate": {"data": "abc"}}}});
        let expr: ReadNodeExpr =
            serde_json::from_value(v).expect("ExprUnforg(Private) should deserialize");
        match expr {
            ReadNodeExpr::ExprUnforg {
                data: ReadNodeExprUnforg::UnforgPrivate { data },
            } => {
                assert_eq!(data, "abc");
            }
            other => panic!("expected ExprUnforg(UnforgPrivate), got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_unforg_deploy() {
        let v = json!({"ExprUnforg": {"data": {"UnforgDeploy": {"data": "dep"}}}});
        let expr: ReadNodeExpr =
            serde_json::from_value(v).expect("ExprUnforg(Deploy) should deserialize");
        match expr {
            ReadNodeExpr::ExprUnforg {
                data: ReadNodeExprUnforg::UnforgDeploy { data },
            } => {
                assert_eq!(data, "dep");
            }
            other => panic!("expected ExprUnforg(UnforgDeploy), got {other:?}"),
        }
    }

    #[test]
    fn deserialize_expr_unforg_deployer() {
        let v = json!({"ExprUnforg": {"data": {"UnforgDeployer": {"data": "dplyr"}}}});
        let expr: ReadNodeExpr =
            serde_json::from_value(v).expect("ExprUnforg(Deployer) should deserialize");
        match expr {
            ReadNodeExpr::ExprUnforg {
                data: ReadNodeExprUnforg::UnforgDeployer { data },
            } => {
                assert_eq!(data, "dplyr");
            }
            other => panic!("expected ExprUnforg(UnforgDeployer), got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Either<L, R>
    // -----------------------------------------------------------------------

    #[test]
    fn either_deserialize_right() {
        let v = json!([true, 42]);
        let either: Either<String, i32> =
            serde_json::from_value(v).expect("Either Right should deserialize");
        assert!(matches!(either, Either::Right(42)), "expected Right(42)");
    }

    #[test]
    fn either_deserialize_left() {
        let v = json!([false, "error message"]);
        let either: Either<String, i32> =
            serde_json::from_value(v).expect("Either Left should deserialize");
        match either {
            Either::Left(msg) => assert_eq!(msg, "error message"),
            Either::Right(n) => panic!("expected Left, got Right({n})"),
        }
    }

    #[test]
    fn either_to_result_ok() {
        let either: Either<String, i32> = Either::Right(100);
        let result = either.to_result();
        assert_eq!(result, Ok(100), "Right should become Ok");
    }

    #[test]
    fn either_to_result_err() {
        let either: Either<String, i32> = Either::Left("fail".to_string());
        let result = either.to_result();
        assert_eq!(result, Err("fail".to_string()), "Left should become Err");
    }

    #[test]
    fn either_from_into_result() {
        let right: Either<&str, u64> = Either::Right(7);
        let result: Result<u64, &str> = right.into();
        assert_eq!(result, Ok(7));

        let left: Either<&str, u64> = Either::Left("nope");
        let result: Result<u64, &str> = left.into();
        assert_eq!(result, Err("nope"));
    }

    // -----------------------------------------------------------------------
    // NodeEvent deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn node_event_started() {
        let v = json!({"event": "started"});
        let event: NodeEvent = serde_json::from_value(v).expect("Started event should deserialize");
        assert!(
            matches!(event, NodeEvent::Started),
            "expected Started variant"
        );
    }

    #[test]
    fn node_event_block_added() {
        let (_sk, pk) = test_keypair();
        let deployer_hex = hex::encode(pk.serialize());

        let v = json!({
            "event": "block-added",
            "payload": {
                "block-hash": "abc123",
                "deploys": [
                    {
                        "id": "deploy-001",
                        "cost": 1000,
                        "deployer": deployer_hex,
                        "errored": false
                    }
                ]
            }
        });

        let event: NodeEvent =
            serde_json::from_value(v).expect("BlockAdded event should deserialize");
        match event {
            NodeEvent::BlockAdded { payload } => {
                assert_eq!(payload.block_hash.as_ref(), "abc123");
                assert_eq!(payload.deploys.len(), 1);
                let deploy = &payload.deploys[0];
                assert_eq!(deploy.id.as_ref(), "deploy-001");
                assert_eq!(deploy.cost, 1000);
                assert_eq!(deploy.deployer, pk);
                assert!(!deploy.errored);
            }
            other => panic!("expected BlockAdded, got {other:?}"),
        }
    }

    #[test]
    fn node_event_block_finalised() {
        let (_sk, pk) = test_keypair();
        let deployer_hex = hex::encode(pk.serialize());

        let v = json!({
            "event": "block-finalised",
            "payload": {
                "block-hash": "finalhash",
                "deploys": [
                    {
                        "id": "deploy-fin-1",
                        "cost": 500,
                        "deployer": deployer_hex,
                        "errored": true
                    }
                ]
            }
        });

        let event: NodeEvent =
            serde_json::from_value(v).expect("BlockFinalised event should deserialize");
        match event {
            NodeEvent::BlockFinalised { payload } => {
                assert_eq!(payload.block_hash.as_ref(), "finalhash");
                assert_eq!(payload.deploys.len(), 1);
                let deploy = &payload.deploys[0];
                assert_eq!(deploy.id.as_ref(), "deploy-fin-1");
                assert_eq!(deploy.cost, 500);
                assert_eq!(deploy.deployer, pk);
                assert!(deploy.errored);
            }
            other => panic!("expected BlockFinalised, got {other:?}"),
        }
    }

    #[test]
    fn node_event_block_finalised_empty_deploys() {
        let v = json!({
            "event": "block-finalised",
            "payload": {
                "block-hash": "emptyhash",
                "deploys": []
            }
        });

        let event: NodeEvent =
            serde_json::from_value(v).expect("BlockFinalised with empty deploys should parse");
        match event {
            NodeEvent::BlockFinalised { payload } => {
                assert!(payload.deploys.is_empty(), "deploys should be empty");
            }
            other => panic!("expected BlockFinalised, got {other:?}"),
        }
    }

    #[test]
    fn node_event_block_created() {
        let v = json!({
            "event": "block-created",
            "payload": {
                "block-hash": "created-hash",
                "deploys": []
            }
        });

        let event: NodeEvent =
            serde_json::from_value(v).expect("BlockCreated event should deserialize");
        assert!(
            matches!(event, NodeEvent::BlockCreated { .. }),
            "expected BlockCreated variant"
        );
    }

    // -----------------------------------------------------------------------
    // DeployData builder
    // -----------------------------------------------------------------------

    #[test]
    fn deploy_data_builder_defaults() {
        let before = chrono::Utc::now();
        let deploy = DeployData::builder("new Nil".to_string()).build();
        let after = chrono::Utc::now();

        assert_eq!(deploy.term, "new Nil", "term should match");
        assert_eq!(
            deploy.phlo_limit, 5_000_000,
            "default phlo_limit should be 5 million"
        );
        assert!(
            deploy.timestamp >= before && deploy.timestamp <= after,
            "timestamp should be approximately now"
        );
        assert!(
            matches!(deploy.valid_after_block_number, ValidAfter::Head),
            "default valid_after should be Head"
        );
    }

    #[test]
    fn deploy_data_builder_custom_values() {
        let custom_time = DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
            .expect("valid rfc3339")
            .with_timezone(&Utc);

        let deploy = DeployData::builder("@0!(true)".to_string())
            .phlo_limit(1_000)
            .timestamp(custom_time)
            .valid_after_block_number(ValidAfter::Index(42))
            .build();

        assert_eq!(deploy.term, "@0!(true)");
        assert_eq!(deploy.phlo_limit, 1_000);
        assert_eq!(deploy.timestamp, custom_time);
        assert!(
            matches!(deploy.valid_after_block_number, ValidAfter::Index(42)),
            "valid_after should be Index(42)"
        );
    }

    #[test]
    fn deploy_data_builder_zero_phlo_limit() {
        let deploy = DeployData::builder("Nil".to_string()).phlo_limit(0).build();
        assert_eq!(deploy.phlo_limit, 0, "phlo_limit of 0 should be allowed");
    }
}
