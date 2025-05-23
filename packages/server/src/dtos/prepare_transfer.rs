use etc::ParseWalletAddressError;
use poem::{error::ResponseError, http::StatusCode};
use poem_openapi::Object;
use thiserror::Error;
use wallet::PrepareTransferInput;

#[derive(Debug, Object)]
pub(crate) struct PrepareTransferInputDto {
    from: String,
    to: String,
    amount: u64,
    description: Option<String>,
}

#[derive(Debug, Error)]
pub enum PrepareContractRequestProblem {
    #[error("Amount field can't be empty")]
    EmptyAmount,
    #[error("Receiver address field can't be empty")]
    EmptyReceiverAddress,
    #[error("Serder address field can't be empty")]
    EmptySenderAddress,
    #[error("Wallet adress has wrong format")]
    WrongAddressFormat(ParseWalletAddressError),
}

impl ResponseError for PrepareContractRequestProblem {
    fn status(&self) -> poem::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl TryFrom<PrepareTransferInputDto> for PrepareTransferInput {
    type Error = PrepareContractRequestProblem;

    fn try_from(value: PrepareTransferInputDto) -> Result<Self, Self::Error> {
        if value.amount == 0 {
            return Err(PrepareContractRequestProblem::EmptyAmount);
        }

        if value.to.is_empty() {
            return Err(PrepareContractRequestProblem::EmptyReceiverAddress);
        }

        if value.from.is_empty() {
            return Err(PrepareContractRequestProblem::EmptySenderAddress);
        }

        let from = value
            .from
            .try_into()
            .map_err(PrepareContractRequestProblem::WrongAddressFormat)?;

        let to = value
            .to
            .try_into()
            .map_err(PrepareContractRequestProblem::WrongAddressFormat)?;

        Ok(PrepareTransferInput {
            from,
            to,
            amount: value.amount,
            description: value.description.map(Into::into),
        })
    }
}
