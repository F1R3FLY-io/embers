use crate::Code;

#[derive(Debug)]
pub struct SignedContract {
    pub contract: Code,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
