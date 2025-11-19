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

    #[error("CEL compilation error: {0}")]
    CelCompilationError(String),

    #[error("CEL execution error: {0}")]
    CelExecutionError(String),

    #[error("invalid JSON output from CEL expression")]
    InvalidJsonOutput,
}

pub type Result<T> = std::result::Result<T, PayloadError>;
