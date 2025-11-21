use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device already exists: {0}")]
    DeviceAlreadyExists(String),

    #[error("Invalid device ID: {0}")]
    InvalidDeviceId(String),

    #[error("Invalid organization ID: {0}")]
    InvalidOrganizationId(String),

    #[error("Invalid device name: {0}")]
    InvalidDeviceName(String),

    #[error("Payload conversion error: {0}")]
    PayloadConversionError(String),

    #[error("Missing CEL expression for device: {0}")]
    MissingCelExpression(String),

    #[error("Repository error: {0}")]
    RepositoryError(#[from] anyhow::Error),
}

pub type DomainResult<T> = Result<T, DomainError>;
