use etc::SignedCode;
use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct TransferSendDto {
    code: String,
    sig: Vec<u8>,
    sig_algorithm: String,
    deployer: Vec<u8>,
}

impl From<TransferSendDto> for SignedCode {
    fn from(value: TransferSendDto) -> Self {
        Self {
            contract: value.code.into(),
            sig: value.sig,
            sig_algorithm: value.sig_algorithm,
            deployer: value.deployer,
        }
    }
}
