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
    #[error("explore-deploy timed out after {0:?}")]
    Timeout(std::time::Duration),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_return_value_missing_display() {
        let err = ReadNodeError::ReturnValueMissing;
        assert_eq!(err.to_string(), "contract did not return any value");
    }

    #[test]
    fn test_api_error_display() {
        let err = ReadNodeError::Api(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "body text".to_string(),
        );
        let display = err.to_string();
        assert!(display.contains("500"), "expected '500' in: {display}");
        assert!(
            display.contains("body text"),
            "expected 'body text' in: {display}"
        );
    }

    #[test]
    fn test_deserialization_error_display() {
        let err = ReadNodeError::Deserialization(anyhow::anyhow!("parse failed"));
        let display = err.to_string();
        assert!(
            display.contains("parse failed"),
            "expected 'parse failed' in: {display}"
        );
    }

    #[test]
    fn test_timeout_error_display() {
        let err = ReadNodeError::Timeout(Duration::from_secs(45));
        let display = err.to_string();
        assert!(display.contains("45"), "expected '45' in: {display}");
    }
}
