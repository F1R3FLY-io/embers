mod description;
mod wallet_address;

use std::num::NonZero;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use self::description::*;
pub use self::wallet_address::*;
use uuid::Uuid;
pub use wallet_address::*;

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

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq)]
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
