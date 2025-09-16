use firefly_client::ReadNodeClient;
use firefly_client::models::Either;
use firefly_client::rendering::Render;

use crate::common::models::WalletAddress;
use crate::common::tracing::record_trace;
use crate::wallets::blockchain::dtos;
use crate::wallets::models::{Direction, Transfer, WalletStateAndHistory};

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/get_balance_and_histry.rho")]
struct GetBalanceAndHistory {
    wallet_address: WalletAddress,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn get_wallet_state_and_history(
    client: &ReadNodeClient,
    address: WalletAddress,
) -> anyhow::Result<WalletStateAndHistory> {
    record_trace!(address);

    let contract = GetBalanceAndHistory {
        wallet_address: address.clone(),
    }
    .render()?;

    let state = client
        .get_data::<Either<String, dtos::BalanceAndHistory>>(contract)
        .await?
        .to_result()
        .map_err(|err| anyhow::anyhow!("error from contract: {err}"))?;

    let transfers: Vec<_> = state
        .history
        .into_iter()
        .flat_map(dtos::Transaction::try_from)
        .map(|operation| {
            let direction = if address == operation.to {
                Direction::Incoming
            } else {
                Direction::Outgoing
            };

            Transfer {
                id: operation.id,
                direction,
                amount: operation.amount,
                date: operation.timestamp,
                to_address: operation.to,
                cost: 0, // Assuming cost is not provided in the operation
            }
        })
        .collect();

    Ok(WalletStateAndHistory {
        balance: state.balance,
        transfers,
        boosts: vec![],
        exchanges: vec![],
        requests: vec![],
    })
}
