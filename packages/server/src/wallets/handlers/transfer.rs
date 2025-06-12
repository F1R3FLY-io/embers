use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

use crate::common::deploy_signed_contract;
use crate::common::models::PreparedContract;
use crate::common::rendering::{PrepareForSigning, RhoValue};
use crate::common::tracing::record_trace;
use crate::wallets::models::PrepareTransferInput;

#[derive(Template)]
#[template(path = "wallets/transfer_contract.rho", escape = "none")]
struct TransferContract {
    wallet_address_from: RhoValue<String>,
    wallet_address_to: RhoValue<String>,
    amount: RhoValue<u64>,
    description: RhoValue<Option<String>>,
}

#[tracing::instrument(level = "info", skip_all, fields(request), ret(Debug, level = "trace"))]
pub fn prepare_transfer_contract(request: PrepareTransferInput) -> PreparedContract {
    record_trace!(request);

    TransferContract {
        wallet_address_from: String::from(request.from).into(),
        wallet_address_to: String::from(request.to).into(),
        amount: request.amount.get().into(),
        description: request.description.map(Into::into).into(),
    }
    .prepare_for_signing()
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
