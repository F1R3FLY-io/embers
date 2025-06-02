use crate::wallet::models::{Id, WalletAddress};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ChainOperationRecord {
    id: Id,
    from: String,
    to: String,
    amount: u64,
    description: String,
}

#[allow(dead_code)]
pub struct OperationRecord {
    pub id: Id,
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: u64,
    pub description: String,
}

pub enum TransformError {
    InvalidFromAddress,
    InvalidToAddress,
}

impl TryFrom<ChainOperationRecord> for OperationRecord {
    type Error = TransformError;

    fn try_from(record: ChainOperationRecord) -> Result<Self, Self::Error> {
        let from =
            WalletAddress::try_from(record.from).map_err(|_| TransformError::InvalidFromAddress)?;
        let to =
            WalletAddress::try_from(record.to).map_err(|_| TransformError::InvalidToAddress)?;

        Ok(Self {
            id: record.id,
            from,
            to,
            amount: record.amount,
            description: record.description,
        })
    }
}
