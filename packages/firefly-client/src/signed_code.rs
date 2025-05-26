#[derive(Debug)]
pub struct SignedCode {
    pub contract: String,
    pub sig: Vec<u8>,
    pub sig_algorithm: String,
    pub deployer: Vec<u8>,
}
