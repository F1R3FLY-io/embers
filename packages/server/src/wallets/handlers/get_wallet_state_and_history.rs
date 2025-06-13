use firefly_client::ReadNodeClient;

use crate::wallets::contracts::{create_check_balance_contract, get_user_history_contract};
use crate::wallets::dtos::{ChainOperationRecord, OperationRecord};
use crate::wallets::models::{Direction, Transfer, WalletAddress, WalletStateAndHistory};
use chrono::{DateTime, Utc};

pub async fn get_wallet_state_and_history(
    client: &ReadNodeClient,
    address: WalletAddress,
) -> anyhow::Result<WalletStateAndHistory> {
    let contract = create_check_balance_contract(&address);
    let balance = client.get_data("/expr/0/ExprInt/data", contract).await?;

    let contract = get_user_history_contract(&address);
    let transfers = client
        .get_data::<Vec<ChainOperationRecord>>("/expr", contract)
        .await?
        .into_iter()
        .flat_map(OperationRecord::try_from)
        .filter_map(|operation| {
            let direction = if address == operation.to {
                Direction::Outgoing
            } else {
                Direction::Incoming
            };

            operation.id.get_timestamp().map(|date| {
                let timestamp = i64::try_from(date.to_unix().0).ok()?;
                DateTime::<Utc>::from_timestamp_millis(timestamp).map(|date| {
                    Transfer {
                        id: operation.id,
                        direction,
                        amount: operation.amount,
                        date,
                        to_address: operation.to,
                        cost: 0, // Assuming cost is not provided in the operation
                    }
                })
            })
        })
        .flatten()
        .collect();

    Ok(WalletStateAndHistory {
        balance,
        transfers,
        ..Default::default()
    })
}
