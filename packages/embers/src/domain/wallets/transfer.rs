use chrono::{DateTime, Utc};
use firefly_client::models::{DeployId, SignedCode, Uri, WalletAddress};
use firefly_client::rendering::Render;

use crate::domain::common::{prepare_for_signing, record_trace};
use crate::domain::wallets::WalletsService;
use crate::domain::wallets::models::{TransferReq, TransferResp};

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/transfer.rho")]
struct TransferContract {
    env_uri: Uri,
    timestamp: DateTime<Utc>,
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
    description: Option<String>,
}

impl WalletsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_transfer_contract(
        &self,
        request: TransferReq,
    ) -> anyhow::Result<TransferResp> {
        record_trace!(request);

        let contract = TransferContract {
            env_uri: self.uri.clone(),
            timestamp: Utc::now(),
            wallet_address_from: request.from,
            wallet_address_to: request.to,
            amount: request.amount.0,
            description: request.description,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        let contract = prepare_for_signing()
            .code(contract)
            .valid_after_block_number(valid_after)
            .call();
        Ok(TransferResp { contract })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_transfer(&self, contract: SignedCode) -> anyhow::Result<DeployId> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;
        write_client.propose().await?;
        Ok(deploy_id)
    }
}
