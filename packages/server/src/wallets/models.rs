mod description;
mod wallet_address;

use std::num::NonZero;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use self::description::*;
pub use self::wallet_address::*;

pub type Amount = NonZero<u64>;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub to_address: WalletAddress,
    pub cost: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone)]
pub struct WalletStateAndHistory {
    pub balance: u64,
    pub requests: Vec<Request>,
    pub exchanges: Vec<Exchange>,
    pub boosts: Vec<Boost>,
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub status: RequestStatus,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Exchange {}

#[derive(Debug, Clone)]
pub struct PrepareTransferInput {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}
