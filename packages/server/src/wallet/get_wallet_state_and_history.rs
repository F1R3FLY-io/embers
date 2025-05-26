use std::error::Error;

use etc::WalletAddress;
use firefly_client::{FireflyClient, models::WalletStateAndHistory};

pub async fn get_wallet_state_and_history(
    client: &FireflyClient,
    address: WalletAddress,
) -> Result<WalletStateAndHistory, Box<dyn Error>> {
    client.get_state_and_history(address).await
}
