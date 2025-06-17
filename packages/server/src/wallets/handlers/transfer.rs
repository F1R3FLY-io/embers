use askama::Template;
use firefly_client::WriteNodeClient;
use firefly_client::models::SignedCode;

use crate::common::models::PreparedContract;
use crate::common::rendering::{PrepareForSigning, RhoValue};
use crate::common::{deploy_signed_contract, generate_id};
use crate::wallets::models::{Id, PrepareTransferInput};

#[derive(Template)]
#[template(path = "wallet/send_tokens.rho", escape = "none")]
struct TransferContract {
    id: RhoValue<Id>,
    wallet_address_from: RhoValue<String>,
    wallet_address_to: RhoValue<String>,
    amount: RhoValue<u64>,
    description: RhoValue<Option<String>>,
}

#[tracing::instrument(level = "info", skip_all)]
#[tracing::instrument(level = "trace", ret(Debug))]
pub fn prepare_transfer_contract(value: PrepareTransferInput) -> PreparedContract {
    TransferContract {
        id: generate_id().into(),
        wallet_address_from: String::from(value.from).into(),
        wallet_address_to: String::from(value.to).into(),
        amount: value.amount.get().into(),
        description: value.description.map(Into::into).into(),
    }
    .prepare_for_signing()
}

#[tracing::instrument(level = "info", skip_all, err(Debug))]
#[tracing::instrument(level = "trace", skip_all, ret(Debug), err(Debug))]
pub async fn deploy_signed_transfer(
    client: &mut WriteNodeClient,
    contract: SignedCode,
) -> anyhow::Result<()> {
    deploy_signed_contract(client, contract).await
}
