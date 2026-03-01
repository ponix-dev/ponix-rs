use crate::domain::DomainError;
use tonic::Status;

/// Convert domain error to gRPC Status
pub fn domain_error_to_status(error: DomainError) -> Status {
    match error {
        DomainError::DataStreamNotFound(msg) => Status::not_found(msg),

        DomainError::DataStreamNotOwnedByOrganization(data_stream_id, org_id) => {
            Status::permission_denied(format!(
                "Data stream {} not owned by organization {}",
                data_stream_id, org_id
            ))
        }

        DomainError::DataStreamAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidDataStreamId(msg)
        | DomainError::InvalidOrganizationId(msg)
        | DomainError::InvalidDataStreamName(msg)
        | DomainError::InvalidOrganizationName(msg) => Status::invalid_argument(msg),

        DomainError::PayloadConversionError(msg) => Status::invalid_argument(msg),

        DomainError::DataStreamDefinitionNotFound(msg) => Status::not_found(msg),

        DomainError::DataStreamDefinitionAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidJsonSchema(msg) => Status::invalid_argument(msg),

        DomainError::SchemaValidationFailed(data_stream_id, reason) => {
            Status::invalid_argument(format!(
                "Schema validation failed for data stream {}: {}",
                data_stream_id, reason
            ))
        }

        DomainError::DataStreamDefinitionInUse(msg) => Status::failed_precondition(msg),

        DomainError::OrganizationNotFound(msg) => Status::not_found(msg),

        DomainError::OrganizationAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::OrganizationDeleted(msg) => Status::failed_precondition(msg),

        DomainError::WorkspaceNotFound(msg) => Status::not_found(msg),

        DomainError::WorkspaceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::WorkspaceDeleted(msg) => Status::failed_precondition(msg),

        DomainError::GatewayNotFound(msg) => Status::not_found(msg),

        DomainError::GatewayAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::DocumentNotFound(msg) => Status::not_found(msg),

        DomainError::DocumentAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidGatewayId(msg)
        | DomainError::InvalidGatewayType(msg)
        | DomainError::InvalidGatewayConfig(msg) => Status::invalid_argument(msg),

        DomainError::UserNotFound(msg) => Status::not_found(msg),

        DomainError::UserAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidUserId(msg)
        | DomainError::InvalidEmail(msg)
        | DomainError::InvalidPassword(msg)
        | DomainError::InvalidUserName(msg) => Status::invalid_argument(msg),

        DomainError::PasswordHashingError(msg) => {
            Status::internal(format!("Password hashing error: {}", msg))
        }

        DomainError::InvalidCredentials => Status::unauthenticated("Invalid email or password"),

        DomainError::InvalidToken(msg) => {
            Status::unauthenticated(format!("Invalid token: {}", msg))
        }

        DomainError::RefreshTokenNotFound => Status::unauthenticated("Refresh token not found"),

        DomainError::RefreshTokenExpired => Status::unauthenticated("Refresh token expired"),

        DomainError::UserOrganizationAlreadyExists(user_id, org_id) => Status::already_exists(
            format!("User {} already in organization {}", user_id, org_id),
        ),

        DomainError::PermissionDenied(msg) => Status::permission_denied(msg),

        DomainError::AuthorizationError(msg) => Status::internal(msg),

        DomainError::RepositoryError(err) => Status::internal(format!("Internal error: {}", err)),

        DomainError::ValidationError(msg) => Status::invalid_argument(msg),
    }
}
