use std::error::Error;

use etc::ParseWalletAddressError;
use poem::{error::ResponseError, http::StatusCode};
use poem_openapi::Object;
use wallet::PrepareTransferInput;

#[derive(Debug, Object)]
pub(crate) struct PrepareTransferInputDto {
    from: String,
    to: String,
    amount: u64,
    description: Option<String>,
}

#[derive(Debug)]
pub enum PrepareContractRequestProblem {
    EmptyAmount,
    EmptyReceiverAddress,
    EmptySenderAddress,
    WrongAddressFormat(ParseWalletAddressError),
}

impl std::fmt::Display for PrepareContractRequestProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self)
    }
}

impl Error for PrepareContractRequestProblem {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn Error> {
        self.source()
    }
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
