use thiserror::Error;

pub type DomainResult<T> = Result<T, DomainError>;

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

    #[error("Organization not found: {0}")]
    OrganizationNotFound(String),

    #[error("Organization already exists: {0}")]
    OrganizationAlreadyExists(String),

    #[error("Invalid organization name: {0}")]
    InvalidOrganizationName(String),

    #[error("Organization is deleted: {0}")]
    OrganizationDeleted(String),

    #[error("Gateway not found: {0}")]
    GatewayNotFound(String),

    #[error("Gateway already exists: {0}")]
    GatewayAlreadyExists(String),

    #[error("Invalid gateway ID: {0}")]
    InvalidGatewayId(String),

    #[error("Invalid gateway type: {0}")]
    InvalidGatewayType(String),

    #[error("Invalid gateway configuration: {0}")]
    InvalidGatewayConfig(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("User already exists: {0}")]
    UserAlreadyExists(String),

    #[error("Invalid user ID: {0}")]
    InvalidUserId(String),

    #[error("Invalid email: {0}")]
    InvalidEmail(String),

    #[error("Invalid password: {0}")]
    InvalidPassword(String),

    #[error("Invalid user name: {0}")]
    InvalidUserName(String),

    #[error("Password hashing error: {0}")]
    PasswordHashingError(String),

    #[error("Repository error: {0}")]
    RepositoryError(#[from] anyhow::Error),
}
