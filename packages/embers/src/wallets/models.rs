use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, WalletAddress};

use crate::common::models::PositiveNonZero;

mod description;

pub use description::*;

pub type Amount = PositiveNonZero<i64>;

pub type Id = String;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: Id,
    pub direction: Direction,
    pub date: DateTime<Utc>,
    pub amount: Amount,
    pub to_address: WalletAddress,
    pub cost: u64,
}

#[derive(Debug, Clone)]
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
pub struct TransferReq {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}

#[derive(Debug, Clone)]
pub enum NodeType {
    Validator,
    Observer,
}

#[derive(Debug, Clone)]
pub struct DeployDescription {
    pub deploy_id: DeployId,
    pub cost: u64,
    pub errored: bool,
    pub node_type: NodeType,
}

#[derive(Debug, Clone)]
pub enum DeployEvent {
    Finalized(DeployDescription),
}
