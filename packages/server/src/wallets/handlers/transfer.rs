use std::num::NonZero;

use firefly_client::models::SignedCode;
use firefly_client::{WriteNodeClient, template};

use crate::common::models::PreparedContract;
use crate::common::tracing::record_trace;
use crate::common::{deploy_signed_contract, prepare_for_signing};
use crate::wallets::models::{PrepareTransferInput, WalletAddress};

template! {
    #[template(path = "wallets/transfer_contract.rho")]
    #[derive(Debug, Clone)]
    struct TransferContract {
        wallet_address_from: WalletAddress,
        wallet_address_to: WalletAddress,
        amount: NonZero<u64>,
        #[allow(dead_code)]
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
pub fn prepare_transfer_contract(
    request: PrepareTransferInput,
) -> anyhow::Result<PreparedContract> {
    record_trace!(request);

    let contract = TransferContract {
        wallet_address_from: request.from,
        wallet_address_to: request.to,
        amount: request.amount,
        description: request.description.map(Into::into),
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
