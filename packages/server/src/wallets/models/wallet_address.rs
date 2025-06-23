use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use derive_more::{AsRef, Into};
use serde::Serialize;
use thiserror::Error;

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
