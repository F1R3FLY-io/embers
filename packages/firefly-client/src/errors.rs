#[derive(Debug, thiserror::Error)]
pub enum ReadNodeError {
    #[error("contract did not return any value")]
    ReturnValueMissing,
    #[error("read node returned error: status {0}, body {1}")]
    Api(reqwest::StatusCode, String),
    #[error("failed to deserialize: {0}")]
    Deserialization(anyhow::Error),
    #[error("http transport error: {0}")]
    Transport(#[from] reqwest::Error),
}
