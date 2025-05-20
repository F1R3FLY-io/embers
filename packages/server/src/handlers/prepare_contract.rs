use poem::{IntoResponse, http::StatusCode};
use poem_openapi::{ApiRequest, Object, payload::Json, types::Type};
use prost::Message;
use sailfish::{RenderError, TemplateSimple};
use std::fmt::Display;

use crate::domain::templates::create_transfer_contract;

pub(crate) type Amount = u64;

#[derive(ApiRequest)]
pub(crate) enum PrepareTransfer {
    CreateByJSON(Json<PrepareTransferInputDto>),
}

#[derive(Object)]
pub(crate) struct PrepareTransferInputDto {
    from: String,
    to: String,
    amount: Amount,
    description: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PrepareTransferInput {
    from: String,
    to: String,
    amount: Amount,
    description: Option<String>,
}

#[derive(Debug, Object)]
pub(crate) struct PreparedContract {
    contract: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum PrepareContractRequestProblem {
    HandlingError(HandlingErrors),
    EmptyAmount,
    EmptyReceiverAddress,
    EmptySenderAddress,
}

impl Display for PrepareContractRequestProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self)
    }
}

impl From<PrepareContractRequestProblem> for poem::Error {
    fn from(value: PrepareContractRequestProblem) -> Self {
        let status = match value {
            PrepareContractRequestProblem::HandlingError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            PrepareContractRequestProblem::EmptyAmount => StatusCode::BAD_REQUEST,
            PrepareContractRequestProblem::EmptyReceiverAddress => StatusCode::BAD_REQUEST,
            PrepareContractRequestProblem::EmptySenderAddress => StatusCode::BAD_REQUEST,
        };
        poem::Error::from_string(value.to_string(), status)
    }
}

impl TryFrom<PrepareTransferInputDto> for PrepareTransferInput {
    type Error = PrepareContractRequestProblem;

    fn try_from(value: PrepareTransferInputDto) -> Result<Self, Self::Error> {
        if value.amount.is_empty() {
            return Err(PrepareContractRequestProblem::EmptyAmount);
        }

        if value.to.is_empty() {
            return Err(PrepareContractRequestProblem::EmptyReceiverAddress);
        }

        if value.from.is_empty() {
            return Err(PrepareContractRequestProblem::EmptySenderAddress);
        }

        Ok(PrepareTransferInput {
            from: value.from,
            to: value.to,
            amount: value.amount,
            description: value.description,
        })
    }
}

impl IntoResponse for PreparedContract {
    fn into_response(self) -> poem::Response {
        poem::Response::builder().body(self.contract)
    }
}

#[derive(Debug, Object)]
pub(crate) struct PreparedContractResponse {
    content: String,
}

#[derive(Debug)]
pub(crate) enum HandlingErrors {
    TemplateRenderError(RenderError),
}

pub(crate) fn handle(value: PrepareTransferInput) -> Result<PreparedContract, HandlingErrors> {
    create_transfer_contract(value.from, value.to, value.amount, value.description)
        .render_once()
        .map(|code| {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let contract = DeployDataProto {
                term: code,
                timestamp,
                phlo_price: 1,
                phlo_limit: 500_000,
                valid_after_block_number: 0,
                shard_id: "root".into(),
                ..Default::default()
            }
            .encode_to_vec();

            PreparedContract { contract }
        })
        .map_err(HandlingErrors::TemplateRenderError)
}
