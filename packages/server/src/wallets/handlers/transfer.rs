use chrono::{DateTime, Utc};
use firefly_client::models::SignedCode;
use firefly_client::{WriteNodeClient, template};

use crate::common::models::{PreparedContract, WalletAddress};
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};
use crate::wallets::models::{Description, PrepareTransferInput};

template! {
    #[template(path = "wallets/send_tokens.rho")]
    #[derive(Debug, Clone)]
    struct TransferContract {
        timestamp: DateTime<Utc>,
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: i64,
        description: Option<Description>,
    }
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(request),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub fn prepare_transfer_contract(value: PrepareTransferInput) -> anyhow::Result<PreparedContract> {
    record_trace!(value);

    let contract = TransferContract {
        timestamp: Utc::now(),
        wallet_address_from: value.from,
        wallet_address_to: value.to,
        amount: value.amount.0,
        description: value.description,
    }
    .render()?;

    Ok(prepare_for_signing(contract))
}

#[tracing::instrument(
    level = "info",
    skip_all,
    fields(contract),
    err(Debug),
    ret(Debug, level = "trace")
)]
pub async fn deploy_signed_transfer(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    record_trace!(contract);

    deploy_signed_contract(client, contract).await
}
