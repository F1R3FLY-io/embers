use askama::Template;
use firefly_client::models::Deploy;
use firefly_client::{BlocksClient, ReadNodeClient};

use crate::common::rendering::RhoValue;
use crate::common::tracing::record_trace;
use crate::wallets::models::{
    Direction,
    Operation,
    Transfer,
    WalletAddress,
    WalletStateAndHistory,
};

#[derive(Debug, Clone, Template)]
#[template(path = "wallets/check_balance.rho", escape = "none")]
struct CheckBalance {
    wallet_address: RhoValue<WalletAddress>,
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(address),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn get_wallet_state_and_history(
    read_client: &ReadNodeClient,
    block_client: &BlocksClient,
    address: WalletAddress,
) -> anyhow::Result<WalletStateAndHistory> {
    record_trace!(address);

    let code = CheckBalance {
        wallet_address: address.clone().into(),
    }
    .render()
    .unwrap();

    let balance = read_client.get_data(code).await?;
    let transfers = block_client
        .get_deploys()
        .await?
        .into_iter()
        .filter(|deploy| !deploy.errored)
        .filter_map(|deploy| {
            deploy
                .term
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("//FIREFLY_OPERATION;"))
                .map(ToOwned::to_owned)
                .map(|meta| (deploy, meta))
        })
        .filter_map(|(deploy, meta)| {
            serde_json::from_str(&meta)
                .map(|operation| (deploy, operation))
                .ok()
        })
        .filter(|(_, operation): &(Deploy, Operation)| {
            matches!(
                operation,
                Operation::Transfer {
                    wallet_address_from,
                    wallet_address_to,
                    ..
                } if wallet_address_from == &address || wallet_address_to == &address
            )
        })
        .filter_map(|(deploy, operation)| {
            let Operation::Transfer {
                wallet_address_from,
                wallet_address_to,
                amount,
                ..
            } = operation;

            let direction = if address == wallet_address_from {
                Direction::Outgoing
            } else {
                Direction::Incoming
            };

            let date = chrono::DateTime::from_timestamp_millis(deploy.timestamp)?;

            Some(Transfer {
                id: deploy.sig,
                direction,
                date,
                amount,
                to_address: wallet_address_to,
                cost: deploy.cost,
            })
        })
        .collect();

    Ok(WalletStateAndHistory {
        balance,
        transfers,
        boosts: vec![],
        exchanges: vec![],
        requests: vec![],
    })
}
