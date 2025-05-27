use firefly_client::WriteNodeClient;
use firefly_client::signed_code::SignedCode;

pub async fn deploy_signed_contract(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    client.deploy_signed_contract(contract).await
}
