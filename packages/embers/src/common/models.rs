mod positive_non_zero;
mod wallet_address;

use chrono::{DateTime, Utc};
use firefly_client::helpers::ShortHex;
use secp256k1::PublicKey;

pub use self::positive_non_zero::*;
pub use self::wallet_address::*;

#[derive(derive_more::Debug, Clone)]
#[debug("{:?}", _0.short_hex(32))]
pub struct PreparedContract(pub Vec<u8>);

#[derive(Debug, Clone)]
pub struct RegistryDeploy {
    pub timestamp: DateTime<Utc>,
    pub version: PositiveNonZero<i64>,
    pub uri_pub_key: PublicKey,
    pub signature: Vec<u8>,
}
