use poem_openapi::Object;

#[derive(derive_more::Debug, Clone, Object)]
pub struct PreparedContract {
    #[debug("\"{}...\"", hex::encode(&contract[..32]))]
    pub contract: Vec<u8>,
}
