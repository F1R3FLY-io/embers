use firefly_client::{WriteNodeClient, template};
use secp256k1::{PublicKey, Secp256k1, SecretKey, rand};

use crate::ai_agents::models::CreateTestwalletResp;
use crate::common::models::WalletAddress;

const TEST_WALLET_BALANCE: i64 = 1_000_000_000;

template! {
    #[template(path = "ai_agents/fund_test_wallet.rho")]
    #[derive(Debug, Clone)]
    struct FundTestWallet {
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: i64,
    }
}

#[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
pub async fn create_test_wallet(
    client: &mut WriteNodeClient,
    service_key: &SecretKey,
) -> anyhow::Result<CreateTestwalletResp> {
    let sk = Secp256k1::new();
    let (test_account_secret_key, test_account_public_key) = sk.generate_keypair(&mut rand::rng());
    let service_address_public_key = PublicKey::from_secret_key(&sk, service_key);

    let code = FundTestWallet {
        wallet_address_from: service_address_public_key.into(),
        wallet_address_to: test_account_public_key.into(),
        amount: TEST_WALLET_BALANCE,
    }
    .render()?;

    client.deploy(service_key, code).await?;
    client.propose().await?;

    Ok(CreateTestwalletResp {
        key: test_account_secret_key,
    })
}
