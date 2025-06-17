use std::num::NonZero;

use serde::Deserialize;
use thiserror::Error;

use crate::wallets::models::{Id, WalletAddress};

#[derive(Debug, Clone, Deserialize)]
pub struct BlockChainTransactionRecord {
    id: Id,
    from: String,
    to: String,
    amount: u64,
    description: Option<String>,
}

#[derive(Debug)]
pub struct OperationRecord {
    pub id: Id,
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: NonZero<u64>,
    pub description: Option<String>,
}

#[derive(Debug, Error)]
pub enum TransformError {
    #[error("Invalid from address")]
    FromAddress,
    #[error("Invalid to address")]
    ToAddress,
    #[error("Invalid amount")]
    Amount,
}

impl TryFrom<BlockChainTransactionRecord> for OperationRecord {
    type Error = TransformError;

    fn try_from(record: BlockChainTransactionRecord) -> Result<Self, Self::Error> {
        let from = WalletAddress::try_from(record.from).map_err(|_| TransformError::FromAddress)?;
        let to = WalletAddress::try_from(record.to).map_err(|_| TransformError::ToAddress)?;

        let amount = NonZero::try_from(record.amount).map_err(|_| TransformError::Amount)?;

        Ok(Self {
            id: record.id,
            from,
            to,
            amount,
            description: record.description,
        })
    }
}
