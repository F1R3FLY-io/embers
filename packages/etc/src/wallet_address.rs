use std::ops::Deref;

use blake2::{Blake2b, Digest, digest::consts::U32};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WalletAddress(String);

impl Deref for WalletAddress {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ToString for WalletAddress {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

#[derive(Debug)]
pub enum ParseWalletAddressError {
    EncoderError(bs58::decode::Error),
    InvalidRevAddressSize,
    InvalidAddress(Vec<u8>),
}

fn validate(rev_bytes: Vec<u8>) -> Result<(), ParseWalletAddressError> {
    let (payload, checksum) = rev_bytes
        .split_at_checked(rev_bytes.len() - 4)
        .ok_or(ParseWalletAddressError::InvalidRevAddressSize)?;

    let checksum = hex::encode(checksum);

    let hash = Blake2b::<U32>::new().chain_update(payload).finalize();
    let checksum_calc = hex::encode(&hash[..4]);

    if checksum != checksum_calc {
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
            .and(Ok(WalletAddress(value)))
    }
}
