use firefly_client::models::Deploy;
use firefly_client::{BlocksClient, ReadNodeClient};

use super::create_check_balance_contract;
use crate::wallet::models::{Direction, Operation, Transfer, WalletAddress, WalletStateAndHistory};

pub async fn get_wallet_state_and_history(
    read_client: &ReadNodeClient,
    block_client: &BlocksClient,
    address: WalletAddress,
) -> anyhow::Result<WalletStateAndHistory> {
    let code = create_check_balance_contract(&address);
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
        .filter_map(|(deploy, meta): (Deploy, String)| {
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
        .filter_map(|(deploy, operation): (Deploy, Operation)| {
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

            #[allow(clippy::cast_possible_wrap)]
            chrono::DateTime::from_timestamp_millis(deploy.timestamp as i64).map(|date| Transfer {
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
        address,
        boosts: vec![],
        exchanges: vec![],
        requests: vec![],
    })
}
