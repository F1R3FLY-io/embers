#[derive(Debug, thiserror::Error)]
pub enum ReadNodeError {
    #[error("contract did not return any value")]
    ReturnValueMissing,
    #[error("http error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("intermediate model deserialization error: {0}")]
    InvalidIntermediateModel(serde_json::Error),
    #[error("final model deserialization error: {0}")]
    InvalidFinalModel(serde_json::Error),
}
