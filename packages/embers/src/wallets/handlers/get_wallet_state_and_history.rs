use firefly_client::models::{Either, WalletAddress};
use firefly_client::rendering::{Render, Uri};

use crate::common::tracing::record_trace;
use crate::wallets::blockchain::dtos;
use crate::wallets::handlers::WalletsService;
use crate::wallets::models::{Direction, Transfer, WalletStateAndHistory};

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/get_balance_and_histry.rho")]
struct GetBalanceAndHistory {
    env_uri: Uri,
    wallet_address: WalletAddress,
}

impl WalletsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn get_wallet_state_and_history(
        &self,
        address: WalletAddress,
    ) -> anyhow::Result<WalletStateAndHistory> {
        record_trace!(address);

        let contract = GetBalanceAndHistory {
            env_uri: self.uri.clone(),
            wallet_address: address.clone(),
        }
        .render()?;

        let state = self
            .read_client
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
}
