use chrono::{DateTime, Utc};
use firefly_client::{ReadNodeClient, template};

use crate::wallets::dtos::{BlockChainTransactionRecord, Transaction};
use crate::wallets::models::{Direction, Transfer, WalletAddress, WalletStateAndHistory};

template! {
    #[template(path = "wallet/check_balance.rho")]
    #[derive(Debug, Clone)]
    struct CheckBalance {
        wallet_address: WalletAddress,
    }
}

template! {
    #[template(path = "wallet/get_transactions_history.rho")]
    #[derive(Debug, Clone)]
    struct GetUserHistory {
        wallet_address: WalletAddress,
    }
}

#[tracing::instrument(level = "trace", skip_all, ret(Debug), err(Debug))]
pub async fn get_wallet_state_and_history(
    client: &ReadNodeClient,
    address: WalletAddress,
) -> anyhow::Result<WalletStateAndHistory> {
    let contract = CheckBalance {
        wallet_address: address.clone(),
    }
    .render()?;
    let balance = client.get_data(contract).await?;

    let contract = GetUserHistory {
        wallet_address: address.clone(),
    }
    .render()?;
    let get_data: Vec<BlockChainTransactionRecord> = client.get_data(contract).await?;

    let transfers: Vec<_> = get_data
        .into_iter()
        .flat_map(Transaction::try_from)
        .filter_map(|operation| {
            let direction = if address == operation.to {
                Direction::Incoming
            } else {
                Direction::Outgoing
            };

            operation.id.get_timestamp().map(|date| {
                let timestamp = i64::try_from(date.to_unix().0).ok()?;
                DateTime::<Utc>::from_timestamp(timestamp, 0).map(|date| {
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
