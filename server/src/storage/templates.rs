use anyhow::Context;
use sailfish::TemplateSimple;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE", tag = "type")]
pub enum Operation {
    Transfer {
        wallet_address_from: String,
        wallet_address_to: String,
        amount: u64,
        description: String,
    },
}

#[derive(TemplateSimple)]
#[template(path = "check_balance.rho")]
#[template(rm_whitespace = true)]
struct CheckBallanceTemplate<'a> {
    wallet_address: &'a str,
}

pub fn check_balance_rho(wallet_address: &str) -> anyhow::Result<String> {
    CheckBallanceTemplate { wallet_address }
        .render_once()
        .context("failed to render check_balance_rho")
}

#[derive(TemplateSimple)]
#[template(path = "set_transfer.rho")]
#[template(rm_whitespace = true)]
struct SetTransferTemplate<'a> {
    wallet_address_from: &'a str,
    wallet_address_to: &'a str,
    amount: u64,
    description: &'a str,
}

pub fn set_transfer_rho(
    wallet_address_from: &str,
    wallet_address_to: &str,
    amount: u64,
    description: Option<&str>,
) -> anyhow::Result<String> {
    SetTransferTemplate {
        wallet_address_from,
        wallet_address_to,
        amount,
        description: description.unwrap_or_default(),
    }
    .render_once()
    .context("failed to render set_transfer_rho")
}
