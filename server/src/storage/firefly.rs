use firefly_api::{BlocksClient, ReadNodeClient, WriteNodeClient};

use super::templates::Operation;
use crate::domain::models::{Cost, Direction, Transfer, WalletStateAndHistory};
use crate::storage::templates::{check_balance_rho, set_transfer_rho};

#[derive(Clone, derive_more::Constructor)]
pub struct FireflyApi {
    read_node: ReadNodeClient,
    write_node: WriteNodeClient,
    blocks_client: BlocksClient,
}

impl FireflyApi {
    pub async fn get_state_and_history(
        &self,
        wallet_address: String,
    ) -> anyhow::Result<WalletStateAndHistory> {
        let check_balance_code = check_balance_rho(&wallet_address)?;
        let balance = self.read_node.get_data(check_balance_code).await?;
        let deploys = self.blocks_client.get_deploys().await?;

        let operations = deploys
            .into_iter()
            .filter_map(|deploy| {
                if deploy.errored {
                    return None;
                }

                deploy
                    .term
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("//FIREFLY_OPERATION;"))
                    .map(ToOwned::to_owned)
                    .map(|meta| (deploy, meta))
            })
            .map(|(deploy, meta)| serde_json::from_str(&meta).map(|operation| (deploy, operation)))
            .collect::<Result<Vec<(_, Operation)>, _>>()?;

        Ok(WalletStateAndHistory {
            balance,
            requests: vec![],
            exchanges: vec![],
            boosts: vec![],
            transfers: operations
                .into_iter()
                .filter(|(_, operation)| {
                    matches!(
                        operation,
                        Operation::Transfer {
                            wallet_address_from,
                            wallet_address_to,
                            ..
                        } if wallet_address_from == &wallet_address || wallet_address_to == &wallet_address
                    )
                })
                .map(|(deploy, operation)| {
                    let Operation::Transfer {
                        wallet_address_from,
                        wallet_address_to,
                        amount,
                        ..
                    } = operation;

                    Transfer {
                        id: deploy.sig,
                        direction: if wallet_address == wallet_address_from {
                            Direction::Outgoing
                        } else {
                            Direction::Incoming
                        },
                        date: deploy.timestamp,
                        amount,
                        to_address: wallet_address_to,
                        cost: deploy.cost,
                    }
                })
                .collect(),
            address: wallet_address,
        })
    }

    pub async fn transfer(
        &mut self,
        amount: u64,
        from_address: &str,
        to_address: &str,
        description: Option<&str>,
    ) -> anyhow::Result<Cost> {
        let set_transfer = set_transfer_rho(from_address, to_address, amount, description)?;
        self.write_node.full_deploy(set_transfer).await?;
        Ok(Cost(0))
    }
}
