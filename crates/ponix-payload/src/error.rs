use thiserror::Error;

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("insufficient data: expected at least {expected} bytes, got {actual}")]
    InsufficientData { expected: usize, actual: usize },

    #[error("unsupported sensor type: {0}")]
    UnsupportedType(u8),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("json serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, PayloadError>;
