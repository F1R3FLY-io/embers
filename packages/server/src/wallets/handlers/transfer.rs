use chrono::{DateTime, Utc};
use firefly_client::models::SignedCode;
use firefly_client::rendering::{Render, Uri};

use crate::common::models::{PreparedContract, WalletAddress};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};
use crate::wallets::handlers::WalletsService;
use crate::wallets::models::{Description, TransferReq};

#[derive(Debug, Clone, Render)]
#[template(path = "wallets/send_tokens.rho")]
struct TransferContract {
    env_uri: Uri,
    timestamp: DateTime<Utc>,
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
    description: Option<Description>,
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
    ) -> anyhow::Result<PreparedContract> {
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
        Ok(prepare_for_signing()
            .code(contract)
            .valid_after_block_number(valid_after)
            .call())
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_transfer(&self, contract: SignedCode) -> anyhow::Result<()> {
        record_trace!(contract);

        deploy_signed_contract(&mut self.write_client.clone(), contract).await?;
        Ok(())
    }
}
