use std::error::Error;

use etc::{Code, SignedContract, WalletAddress};

use crate::{
    BlocksClient, ReadNodeClient, WriteNodeClient,
    models::{Deploy, Direction, Operation, Transfer, WalletStateAndHistory},
};

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

    fn create_check_balance_contract(address: &WalletAddress) -> Code {
        (include_str!("check_balance.rho"))
            .replace("<%= &wallet_address %>", address.to_string().as_str())
            .into()
    }

    pub async fn get_state_and_history(
        &self,
        address: WalletAddress,
    ) -> Result<WalletStateAndHistory, Box<dyn Error>> {
        let code = Self::create_check_balance_contract(&address);
        let balance = self.read_client.get_data(code).await?;
        let transfers = self
            .blocks_client
            .get_deploys()
            .await?
            .into_iter()
            .filter(|deploy| !deploy.errored)
            .map(|deploy| {
                deploy
                    .term
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("//FIREFLY_OPERATION;"))
                    .map(ToOwned::to_owned)
                    .map(|meta| (deploy, meta))
            })
            .flatten()
            .map(|(deploy, meta): (Deploy, String)| {
                serde_json::from_str(&meta)
                    .map(|operation| (deploy, operation))
                    .ok()
            })
            .flatten()
            .filter(|(_, operation): &(Deploy, Operation)| {
                matches!(
                    operation,
                    Operation::Transfer {
                        wallet_address_from,
                        wallet_address_to,
                        ..
                    } if wallet_address_from == address || wallet_address_to == address
                )
            })
            .map(|(deploy, operation): (Deploy, Operation)| {
                let Operation::Transfer {
                    wallet_address_from,
                    wallet_address_to,
                    amount,
                    ..
                } = operation;

                let direction = if &address == wallet_address_from {
                    Direction::Outgoing
                } else {
                    Direction::Incoming
                };

                Transfer {
                    id: deploy.sig,
                    direction,
                    date: deploy.timestamp.to_string(),
                    amount: amount.to_string(),
                    to_address: wallet_address_to,
                    cost: deploy.cost.to_string(),
                }
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

    pub async fn deploy_signed_contract(
        &mut self,
        contract: SignedContract,
    ) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
        self.write_client.deploy_signed_contract(contract).await
    }
}
