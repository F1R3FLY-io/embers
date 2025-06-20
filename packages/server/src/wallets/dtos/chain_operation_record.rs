use std::num::NonZero;

use serde::Deserialize;

use crate::wallets::dtos::TransferValidationError;
use crate::wallets::models::{Description, Id, WalletAddress};

#[derive(Debug, Clone, Deserialize)]
pub struct BlockChainTransactionRecord {
    id: Id,
    from: String,
    to: String,
    amount: u64,
    description: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Transaction {
    pub id: Id,
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: NonZero<u64>,
    pub description: Option<Description>,
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
