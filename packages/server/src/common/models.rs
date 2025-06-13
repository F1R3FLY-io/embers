use poem_openapi::Object;

#[derive(Debug, Clone, Object)]
pub struct PreparedContract {
    pub contract: Vec<u8>,
}
