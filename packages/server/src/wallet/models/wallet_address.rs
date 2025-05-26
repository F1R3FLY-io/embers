use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use derive_more::Display;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Display)]
pub struct WalletAddress(String);

#[derive(Debug, Error)]
pub enum ParseWalletAddressError {
    #[error("Internal encoder error")]
    EncoderError(bs58::decode::Error),

    #[error("Invalid address size")]
    InvalidRevAddressSize,

    #[error("Invalid address format")]
    InvalidAddress(Vec<u8>),
}

fn validate(rev_bytes: Vec<u8>) -> Result<(), ParseWalletAddressError> {
    let (payload, checksum) = rev_bytes
        .split_at_checked(rev_bytes.len() - 4)
        .ok_or(ParseWalletAddressError::InvalidRevAddressSize)?;

    let hash = Blake2b::<U32>::new().chain_update(payload).finalize();

    if checksum != &hash[..4] {
        return Err(ParseWalletAddressError::InvalidAddress(rev_bytes));
    }

    Ok(())
}

impl TryFrom<String> for WalletAddress {
    type Error = ParseWalletAddressError;

    fn try_from(value: String) -> Result<WalletAddress, Self::Error> {
        bs58::decode(&value)
            .into_vec()
            .map_err(ParseWalletAddressError::EncoderError)
            .and_then(validate)
            .map(|()| html_escape::encode_safe(&value).to_string())
            .map(WalletAddress)
    }
}
