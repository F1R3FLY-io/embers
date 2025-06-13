use std::num::NonZero;

use askama::Template;
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use chrono::{DateTime, Utc};
use derive_more::{AsRef, Into};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type Amount = NonZero<u64>;

pub type Id = Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct Transfer {
    pub id: Id,
    pub direction: Direction,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub to_address: WalletAddress,
    pub cost: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WalletStateAndHistory {
    pub balance: u64,
    pub requests: Vec<Request>,
    pub exchanges: Vec<Exchange>,
    pub boosts: Vec<Boost>,
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Boost {
    pub id: String,
    pub username: String,
    pub direction: Direction,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub post: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "UPPERCASE")]
pub enum Operation {
    Transfer {
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: Amount,
        description: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub id: String,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub status: RequestStatus,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Exchange {}

#[derive(Debug, Clone)]
pub struct PrepareTransferInput {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}

#[derive(Debug, Clone, Default, Into, AsRef)]
pub struct Description(String);

const MAX_DESCRIPTION_CHARS_COUNT: usize = 512;

#[derive(Debug, Clone, Error)]
pub enum DescriptionError {
    #[error("Maximum description length reached")]
    TooLong,
}

impl TryFrom<String> for Description {
    type Error = DescriptionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_DESCRIPTION_CHARS_COUNT {
            return Result::Err(DescriptionError::TooLong);
        }

        let description = html_escape::encode_safe(&value).into_owned();
        Ok(Self(description))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Into, AsRef)]
pub struct WalletAddress(String);

#[derive(Debug, Clone, Error)]
pub enum ParseWalletAddressError {
    #[error("Internal encoder error: {0}")]
    EncoderError(bs58::decode::Error),

    #[error("Invalid address size")]
    InvalidRevAddressSize,

    #[error("Invalid address format: {0}")]
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
            .ok_or(ParseWalletAddressError::InvalidRevAddressSize)?;

        let hash = Blake2b::<U32>::new().chain_update(payload).finalize();

        if checksum != &hash[..4] {
            return Err(ParseWalletAddressError::InvalidAddress(value));
        }

        Ok(Self(value))
    }
}

impl<'a> From<&'a WalletAddress> for &'a str {
    fn from(value: &'a WalletAddress) -> Self {
        value.0.as_str()
    }
}

#[derive(Template)]
#[template(path = "wallet/init.rho", escape = "none")]
pub struct InitWalletEnv;
