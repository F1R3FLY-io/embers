use std::error::Error;

use etc::SignedCode;
use firefly_client::FireflyClient;

pub async fn deploy_signed_contract(
    client: &mut FireflyClient,
    contract: SignedCode,
) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
    client.deploy_signed_contract(contract).await
}
