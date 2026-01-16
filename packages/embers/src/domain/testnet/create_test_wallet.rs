use firefly_client::models::WalletAddress;
use firefly_client::rendering::Render;
use secp256k1::{PublicKey, Secp256k1, rand};

use crate::domain::testnet::TestnetService;
use crate::domain::testnet::models::CreateTestwalletResp;

const TEST_WALLET_BALANCE: i64 = 1_000_000_000;

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/fund_test_wallet.rho")]
struct FundTestWallet {
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
}

impl TestnetService {
    #[tracing::instrument(level = "info", skip_all, err(Debug), ret(Debug, level = "trace"))]
    pub async fn create_test_wallet(&self) -> anyhow::Result<CreateTestwalletResp> {
        let sk = Secp256k1::new();
        let (test_account_secret_key, test_account_public_key) =
            sk.generate_keypair(&mut rand::rng());
        let service_address_public_key = PublicKey::from_secret_key(&sk, &self.service_key);

        let deploy_data = FundTestWallet {
            wallet_address_from: service_address_public_key.into(),
            wallet_address_to: test_account_public_key.into(),
            amount: TEST_WALLET_BALANCE,
        }
        .builder()?
        .build();

        let mut write_client = self.write_client.clone();
        write_client.deploy(&self.service_key, deploy_data).await?;
        write_client.propose().await?;

        Ok(CreateTestwalletResp {
            key: test_account_secret_key,
        })
    }
}
