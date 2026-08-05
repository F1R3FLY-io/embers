use chrono::{DateTime, Utc};
use firefly_client::models::WalletAddress;
use poem_openapi::{Enum, Object, Union};
use structural_convert::StructuralConvert;

use crate::api::common::{PreparedContract, Stringified};
use crate::domain::common::PositiveNonZero;
use crate::domain::wallets::models;

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Transfer))]
pub struct Transfer {
    /// Deploy identifier of the transfer.
    pub id: String,
    /// When the transfer contract was prepared.
    pub timestamp: Stringified<DateTime<Utc>>,
    /// Sender address.
    pub from: Stringified<WalletAddress>,
    /// Recipient address.
    pub to: Stringified<WalletAddress>,
    /// Transferred amount in base units.
    pub amount: Stringified<PositiveNonZero<i64>>,
    /// Free-text memo carried by the transfer.
    pub description: Option<String>,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Boost))]
pub struct Boost {
    /// Deploy identifier of the boost.
    pub id: String,
    /// When the boost contract was prepared.
    pub timestamp: Stringified<DateTime<Utc>>,
    /// Sender address.
    pub from: Stringified<WalletAddress>,
    /// Recipient address (the post author's wallet).
    pub to: Stringified<WalletAddress>,
    /// Transferred amount in base units.
    pub amount: Stringified<PositiveNonZero<i64>>,
    /// Free-text memo carried by the boost.
    pub description: Option<String>,
    /// DID of the boosted post's author.
    pub post_author_did: String,
    /// Identifier of the boosted post.
    pub post_id: Option<String>,
}

/// Reserved for the exchange history; the wallet state reports none yet.
#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Exchange))]
pub struct Exchange {}

/// Status of a reserved wallet-request entry.
#[derive(Debug, Clone, Eq, PartialEq, StructuralConvert, Enum)]
#[convert(from(models::RequestStatus))]
#[oai(rename_all = "lowercase")]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

/// Reserved for the request history; the wallet state reports none yet.
#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::Request))]
pub struct Request {
    /// Deploy identifier of the request.
    pub id: String,
    /// When the request was prepared.
    pub timestamp: Stringified<DateTime<Utc>>,
    /// Requested amount in base units.
    pub amount: Stringified<PositiveNonZero<i64>>,
    /// Current request status.
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::WalletStateAndHistory))]
pub struct WalletStateAndHistory {
    /// Current balance in base units.
    pub balance: Stringified<u64>,
    /// Reserved; always `[]`.
    pub requests: Vec<Request>,
    /// Reserved; always `[]`.
    pub exchanges: Vec<Exchange>,
    /// Boost history (both directions).
    pub boosts: Vec<Boost>,
    /// Transfer history (both directions).
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone, Hash, Object, StructuralConvert)]
#[convert(into(models::TransferReq))]
pub struct TransferReq {
    /// Sender address; must be the signing wallet's address.
    pub from: Stringified<WalletAddress>,
    /// Recipient address.
    pub to: Stringified<WalletAddress>,
    /// Amount in base units; must be positive.
    pub amount: Stringified<PositiveNonZero<i64>>,
    /// Free-text memo, recorded on chain in the transfer history.
    pub description: Option<String>,
}

#[derive(Debug, Clone, Hash, Object, StructuralConvert)]
#[convert(from(models::TransferResp))]
pub struct TransferResp {
    /// The unsigned transfer contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Hash, Object, StructuralConvert)]
#[convert(into(models::BoostReq))]
pub struct BoostReq {
    /// Sender address; must be the signing wallet's address.
    pub from: Stringified<WalletAddress>,
    /// Recipient address (the post author's wallet).
    pub to: Stringified<WalletAddress>,
    /// Amount in base units; must be positive.
    pub amount: Stringified<PositiveNonZero<i64>>,
    /// Free-text memo.
    pub description: Option<String>,
    /// DID of the boosted post's author.
    pub post_author_did: String,
    /// Identifier of the boosted post.
    pub post_id: Option<String>,
}

#[derive(Debug, Clone, Hash, Object, StructuralConvert)]
#[convert(from(models::BoostResp))]
pub struct BoostResp {
    /// The unsigned boost contract bytes to sign.
    pub contract: PreparedContract,
}

#[derive(Debug, Clone, Enum, StructuralConvert)]
#[convert(from(models::NodeType))]
pub enum NodeType {
    Validator,
    Observer,
}

#[derive(Debug, Clone, Object, StructuralConvert)]
#[convert(from(models::DeployDescription))]
pub struct DeployDescription {
    pub deploy_id: String,
    pub cost: Stringified<u64>,
    pub errored: bool,
    pub node_type: NodeType,
}

#[derive(Debug, Clone, Union, StructuralConvert)]
#[oai(discriminator_name = "type")]
#[convert(from(models::DeployEvent))]
pub enum DeployEvent {
    Finalized(DeployDescription),
}
