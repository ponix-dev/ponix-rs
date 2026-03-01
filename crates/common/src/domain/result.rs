use thiserror::Error;

pub type DomainResult<T> = Result<T, DomainError>;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Data stream not found: {0}")]
    DataStreamNotFound(String),

    #[error("Data stream {0} not owned by organization {1}")]
    DataStreamNotOwnedByOrganization(String, String),

    #[error("Data stream already exists: {0}")]
    DataStreamAlreadyExists(String),

    #[error("Invalid data stream ID: {0}")]
    InvalidDataStreamId(String),

    #[error("Invalid organization ID: {0}")]
    InvalidOrganizationId(String),

    #[error("Invalid data stream name: {0}")]
    InvalidDataStreamName(String),

    #[error("Payload conversion error: {0}")]
    PayloadConversionError(String),

    #[error("Data stream definition not found: {0}")]
    DataStreamDefinitionNotFound(String),

    #[error("Data stream definition already exists: {0}")]
    DataStreamDefinitionAlreadyExists(String),

    #[error("Invalid JSON Schema: {0}")]
    InvalidJsonSchema(String),

    #[error("Schema validation failed for data stream {0}: {1}")]
    SchemaValidationFailed(String, String),

    #[error("Data stream definition in use: {0}")]
    DataStreamDefinitionInUse(String),

    #[error("Organization not found: {0}")]
    OrganizationNotFound(String),

    #[error("Organization already exists: {0}")]
    OrganizationAlreadyExists(String),

    #[error("Invalid organization name: {0}")]
    InvalidOrganizationName(String),

    #[error("Organization is deleted: {0}")]
    OrganizationDeleted(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Workspace already exists: {0}")]
    WorkspaceAlreadyExists(String),

    #[error("Workspace is deleted: {0}")]
    WorkspaceDeleted(String),

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

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid or expired token: {0}")]
    InvalidToken(String),

    #[error("Refresh token not found")]
    RefreshTokenNotFound,

    #[error("Refresh token expired")]
    RefreshTokenExpired,

    #[error("User organization link already exists: user {0} in org {1}")]
    UserOrganizationAlreadyExists(String, String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Authorization error: {0}")]
    AuthorizationError(String),

    #[error("Repository error: {0}")]
    RepositoryError(#[from] anyhow::Error),

    #[error("Validation error: {0}")]
    ValidationError(String),
}
