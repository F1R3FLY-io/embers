use poem_openapi::Object;
use wallet::PreparedContract;

#[derive(Debug, Object)]
pub(crate) struct PreparedContractDto {
    contract: Vec<u8>,
}

impl From<PreparedContract> for PreparedContractDto {
    fn from(value: PreparedContract) -> Self {
        Self {
            contract: value.contract,
        }
    }
}
