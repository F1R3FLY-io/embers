use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use derive_more::{AsRef, Into};
use secp256k1::PublicKey;
use serde::Serialize;
use thiserror::Error;

pub const FIRECAP_ID: [u8; 3] = [0, 0, 0];
pub const FIRECAP_VERSION: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Into, AsRef)]
pub struct WalletAddress(String);

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
