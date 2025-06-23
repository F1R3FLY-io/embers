use std::num::NonZero;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use thiserror::Error;

use crate::wallets::models::{
    Description,
    DescriptionError,
    Id,
    ParseWalletAddressError,
    WalletAddress,
};

#[derive(Debug, Clone, Deserialize)]
pub struct BlockChainTransactionRecord {
    id: String,
    timestamp: DateTime<Utc>,
    from: String,
    to: String,
    amount: u64,
    description: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Transaction {
    pub id: Id,
    pub timestamp: DateTime<Utc>,
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: NonZero<u64>,
    pub description: Option<Description>,
}

#[derive(Debug, Clone, Error)]
pub enum TransferValidationError {
    #[error("amount field can't be empty")]
    EmptyAmount,
    #[error("receiver wallet adress has wrong format: {0}")]
    WrongReceiverAddressFormat(ParseWalletAddressError),
    #[error("sender wallet adress has wrong format: {0}")]
    WrongSenderAddressFormat(ParseWalletAddressError),
    #[error("description format error: {0}")]
    DescriptionError(#[from] DescriptionError),
}

impl TryFrom<BlockChainTransactionRecord> for Transaction {
    type Error = TransferValidationError;

    fn try_from(record: BlockChainTransactionRecord) -> Result<Self, Self::Error> {
        let from = record
            .from
            .try_into()
            .map_err(Self::Error::WrongSenderAddressFormat)?;
        let to = record
            .to
            .try_into()
            .map_err(Self::Error::WrongReceiverAddressFormat)?;

        let amount = record
            .amount
            .try_into()
            .map_err(|_| Self::Error::EmptyAmount)?;
        let description = record.description.map(TryFrom::try_from).transpose()?;

        Ok(Self {
            id: record.id,
            timestamp: record.timestamp,
            from,
            to,
            amount,
            description,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BalanceAndHistory {
    pub balance: u64,
    pub history: Vec<BlockChainTransactionRecord>,
}
