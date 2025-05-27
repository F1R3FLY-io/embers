#[derive(Debug)]
pub struct SignedCode {
    pub contract: Vec<u8>,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
