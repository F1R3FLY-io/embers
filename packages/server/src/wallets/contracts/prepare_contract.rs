use derive_more::{AsRef, Display, Into};
use firefly_client::models::casper::DeployDataProto;
use prost::Message as _;
use thiserror::Error;

use crate::wallet::dtos::PrepareTransferInputDto;
use crate::wallet::handlers::create_transfer_contract;
use crate::wallet::models::{Amount, Id, ParseWalletAddressError, WalletAddress};

#[derive(Debug, Display, Default, Into, AsRef)]
pub struct Description(String);

const MAX_DESCRIPTION_CHARS_COUNT: usize = 512;

#[derive(Debug, Error)]
pub enum DescriptionError {
    #[error("Maximum chars length")]
    TooLong,
}

impl TryFrom<String> for Description {
    type Error = DescriptionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_DESCRIPTION_CHARS_COUNT {
            return Result::Err(Self::Error::TooLong);
        }

        Ok(Self(html_escape::encode_safe(&value).into_owned()))
    }
}

#[derive(Debug)]
pub struct PrepareTransferInput {
    pub from: WalletAddress,
    pub to: WalletAddress,
    pub amount: Amount,
    pub description: Option<Description>,
}

#[derive(Debug, Error)]
pub enum PrepareContractRequestProblem {
    #[error("Amount field can't be empty")]
    EmptyAmount,
    #[error("Receiver wallet adress has wrong format: {0}")]
    WrongReceiverAddressFormat(ParseWalletAddressError),
    #[error("Sender wallet adress has wrong format: {0}")]
    WrongSenderAddressFormat(ParseWalletAddressError),
    #[error("Description format error: {0}")]
    DescriptionError(#[from] DescriptionError),
}

impl TryFrom<PrepareTransferInputDto> for PrepareTransferInput {
    type Error = PrepareContractRequestProblem;

    fn try_from(value: PrepareTransferInputDto) -> Result<Self, Self::Error> {
        let to = value
            .to
            .try_into()
            .map_err(Self::Error::WrongReceiverAddressFormat)?;

        let from = value
            .from
            .try_into()
            .map_err(Self::Error::WrongSenderAddressFormat)?;

        let amount = Amount::try_from(value.amount.0).map_err(|_| Self::Error::EmptyAmount)?;
        let description = value.description.map(Description::try_from).transpose()?;

        Ok(Self {
            from,
            to,
            amount,
            description,
        })
    }
}

#[derive(Debug)]
pub struct PreparedContract {
    pub contract: Vec<u8>,
}

pub fn prepare_contract<F>(id_generator: F, value: PrepareTransferInput) -> PreparedContract
where
    F: Fn() -> Id,
{
    let id = id_generator();

    let term = create_transfer_contract(
        id,
        &value.from,
        &value.to,
        value.amount,
        &value.description.unwrap_or_default(),
    );

    let timestamp = chrono::Utc::now().timestamp_millis();
    let contract = DeployDataProto {
        term,
        timestamp,
        phlo_price: 1,
        phlo_limit: 500_000,
        valid_after_block_number: 0,
        shard_id: "root".into(),
        ..Default::default()
    }
    .encode_to_vec();

    PreparedContract { contract }
}
