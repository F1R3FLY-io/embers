mod positive_non_zero;
mod wallet_address;

use firefly_client::helpers::ShortHex;
pub use positive_non_zero::*;
pub use wallet_address::*;

#[derive(derive_more::Debug, Clone)]
#[debug("{:?}", _0.short_hex(32))]
pub struct PreparedContract(pub Vec<u8>);
