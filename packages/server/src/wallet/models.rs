mod wallet_address;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
pub use wallet_address::*;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: u64,
    pub amount: u64,
    pub to_address: WalletAddress,
    pub cost: String,
}

#[derive(Debug, Clone)]
pub enum Direction {
    Incoming,
    Outgoing,
}

type Balance = u64;

#[derive(Debug, Clone)]
pub struct WalletStateAndHistory {
    pub address: WalletAddress,
    pub balance: Balance,
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
    pub amount: u64,
    pub post: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Transfer {
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: u64,
        description: String,
    },
}

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub date: DateTime<Utc>,
    pub amount: u64,
    pub status: RequestStatus,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Exchange {}
