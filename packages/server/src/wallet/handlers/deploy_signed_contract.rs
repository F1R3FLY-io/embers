use std::error::Error;

use firefly_client::{WriteNodeClient, signed_code::SignedCode};

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
    client.deploy_signed_contract(contract).await
}
