use chrono::{DateTime, Utc};
use serde::Deserialize;
use thiserror::Error;

use crate::common::models::{ParseWalletAddressError, WalletAddress};
use crate::wallets::models::{
    Amount,
    Description,
    DescriptionError,
    Id,
    PositiveNonZeroParsingError,
};

#[derive(Debug, Clone, Deserialize)]
pub struct BlockChainTransactionRecord {
    id: String,
    timestamp: DateTime<Utc>,
    from: String,
    to: String,
    amount: i64,
    description: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Transaction {
    pub id: Id,
    pub timestamp: DateTime<Utc>,
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}

#[derive(Debug, Clone, Error)]
pub enum TransferValidationError {
    #[error("description format error: {0}")]
    AmountError(#[from] PositiveNonZeroParsingError),
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

        let amount = record.amount.try_into()?;
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
