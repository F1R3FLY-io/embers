use etc::{Stringified, WalletAddress};

use crate::{
    BlocksClient, ReadNodeClient, WriteNodeClient,
    models::{Direction, Operation, Transfer, WalletStateAndHistory},
};

pub type Contract = String;

#[derive(Clone)]
pub struct FireflyClient {
    read_client: ReadNodeClient,
    write_client: WriteNodeClient,
    blocks_client: BlocksClient,
}

impl FireflyClient {
    pub fn new(
        read_client: ReadNodeClient,
        write_client: WriteNodeClient,
        blocks_client: BlocksClient,
    ) -> Self {
        Self {
            read_client,
            write_client,
            blocks_client,
        }
    }

    fn create_check_balance_contract(address: &WalletAddress) -> Contract {
        (include_str!("check_balance.rho"))
            .replace("<%= &wallet_address %>", address.to_string().as_str())
    }

    pub async fn get_state_and_history(
        &self,
        wallet_address: WalletAddress,
    ) -> anyhow::Result<WalletStateAndHistory> {
        let check_balance_contract = Self::create_check_balance_contract(&wallet_address);
        let balance = self
            .read_client
            .get_data::<Stringified>(check_balance_contract)
            .await?;
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
            .collect::<Result<Vec<(_, _)>, _>>()?;

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
                        } if wallet_address_from == wallet_address || wallet_address_to == wallet_address
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
                        date: deploy.timestamp.to_string(),
                        amount: amount.to_string(),
                        to_address: wallet_address_to,
                        cost: deploy.cost.to_string(),
                    }
                })
                .collect(),
            address: wallet_address,
        })
    }

    pub async fn deploy(
        &mut self,
        code: String,
        sig: Vec<u8>,
        sig_algorithm: String,
        deployer: Vec<u8>,
    ) -> anyhow::Result<String> {
        self.write_client
            .deploy(code, sig, sig_algorithm, deployer)
            .await
    }
}
