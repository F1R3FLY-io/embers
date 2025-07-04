mod wallet_address;

pub use wallet_address::*;

#[derive(derive_more::Debug, Clone)]
#[debug("\"{}...\"", hex::encode(&_0[..32]))]
pub struct PreparedContract(pub Vec<u8>);
