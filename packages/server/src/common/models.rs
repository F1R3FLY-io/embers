mod wallet_address;

pub use wallet_address::*;

#[derive(derive_more::Debug, Clone)]
pub struct PreparedContract {
    #[debug("\"{}...\"", hex::encode(&contract[..32]))]
    pub contract: Vec<u8>,
}
