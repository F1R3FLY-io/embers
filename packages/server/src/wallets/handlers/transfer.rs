use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

use crate::common::deploy_signed_contract;
use crate::common::models::PreparedContract;
use crate::common::rendering::{PrepareForSigning, RhoValue};
use crate::wallets::models::PrepareTransferInput;

#[derive(Template)]
#[template(path = "wallets/transfer_contract.rho", escape = "none")]
struct TransferContract {
    wallet_address_from: RhoValue<String>,
    wallet_address_to: RhoValue<String>,
    amount: RhoValue<u64>,
    description: RhoValue<Option<String>>,
}

#[tracing::instrument(level = "info", skip_all)]
#[tracing::instrument(level = "trace", ret(Debug))]
pub fn prepare_transfer_contract(value: PrepareTransferInput) -> PreparedContract {
    TransferContract {
        wallet_address_from: String::from(value.from).into(),
        wallet_address_to: String::from(value.to).into(),
        amount: value.amount.get().into(),
        description: value.description.map(Into::into).into(),
    }
    .prepare_for_signing()
}

#[tracing::instrument(level = "info", skip_all, err(Debug))]
#[tracing::instrument(level = "trace", skip(client), ret(Debug))]
pub async fn deploy_signed_transfer(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    deploy_signed_contract(client, contract).await
}
