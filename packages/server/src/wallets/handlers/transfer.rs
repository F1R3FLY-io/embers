use firefly_client::models::SignedCode;
use firefly_client::{WriteNodeClient, template};
use uuid::Uuid;

use crate::common::models::PreparedContract;
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};
use crate::wallets::models::{Id, PrepareTransferInput, WalletAddress};

template! {
    #[template(path = "wallet/send_tokens.rho")]
    #[derive(Debug, Clone)]
    struct TransferContract {
        id: Id,
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: u64,
        description: Option<String>,
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
        id: Uuid::now_v7(),
        wallet_address_from: value.from,
        wallet_address_to: value.to,
        amount: value.amount.get(),
        description: value.description.map(Into::into),
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
