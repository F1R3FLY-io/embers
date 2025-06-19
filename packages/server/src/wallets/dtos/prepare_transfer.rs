use poem::error::ResponseError;
use poem::http::StatusCode;
use poem_openapi::Object;
use thiserror::Error;

use crate::common::dtos::Stringified;
use crate::wallets::models::{DescriptionError, ParseWalletAddressError, PrepareTransferInput};

#[derive(Debug, Clone, Object)]
pub struct PrepareTransferInputDto {
    from: String,
    to: String,
    amount: Stringified<u64>,
    description: Option<String>,
}

#[derive(Debug, Clone, Error)]
pub enum TransformPrepareTransferInputError {
    #[error("amount field can't be empty")]
    EmptyAmount,
    #[error("receiver wallet adress has wrong format: {0}")]
    WrongReceiverAddressFormat(ParseWalletAddressError),
    #[error("sender wallet adress has wrong format: {0}")]
    WrongSenderAddressFormat(ParseWalletAddressError),
    #[error("description format error: {0}")]
    DescriptionError(#[from] DescriptionError),
}

impl ResponseError for TransformPrepareTransferInputError {
    fn status(&self) -> poem::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl TryFrom<PrepareTransferInputDto> for PrepareTransferInput {
    type Error = TransformPrepareTransferInputError;

    fn try_from(value: PrepareTransferInputDto) -> Result<Self, Self::Error> {
        let to = value
            .to
            .try_into()
            .map_err(Self::Error::WrongReceiverAddressFormat)?;

        let from = value
            .from
            .try_into()
            .map_err(Self::Error::WrongSenderAddressFormat)?;

        let amount = value
            .amount
            .0
            .try_into()
            .map_err(|_| Self::Error::EmptyAmount)?;
        let description = value.description.map(TryFrom::try_from).transpose()?;

        Ok(Self {
            from,
            to,
            amount,
            description,
        })
    }
}
