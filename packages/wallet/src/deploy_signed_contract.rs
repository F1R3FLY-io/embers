use std::error::Error;

use etc::SignedContract;
use firefly_client::FireflyClient;

pub async fn deploy_signed_contract(
    client: &mut FireflyClient,
    contract: SignedContract,
) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
    client.deploy_signed_contract(contract).await
}
