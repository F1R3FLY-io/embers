use firefly_client::signed_code::SignedCode;
use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct TransferSendDto {
    code: Vec<u8>,
    sig: Vec<u8>,
    sig_algorithm: String,
    deployer: Vec<u8>,
}

impl From<TransferSendDto> for SignedCode {
    fn from(value: TransferSendDto) -> Self {
        Self {
            contract: value.code,
            sig: value.sig,
            sig_algorithm: value.sig_algorithm,
            deployer: value.deployer,
        }
    }
}
