use crate::domain::DomainError;
use tonic::Status;

/// Convert domain error to gRPC Status
pub fn domain_error_to_status(error: DomainError) -> Status {
    match error {
        DomainError::DeviceNotFound(msg) => Status::not_found(msg),

        DomainError::DeviceNotOwnedByOrganization(device_id, org_id) => Status::permission_denied(
            format!("Device {} not owned by organization {}", device_id, org_id),
        ),

        DomainError::DeviceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidDeviceId(msg)
        | DomainError::InvalidOrganizationId(msg)
        | DomainError::InvalidDeviceName(msg)
        | DomainError::InvalidOrganizationName(msg) => Status::invalid_argument(msg),

        DomainError::PayloadConversionError(msg) => Status::invalid_argument(msg),

        DomainError::EndDeviceDefinitionNotFound(msg) => Status::not_found(msg),

        DomainError::EndDeviceDefinitionAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidJsonSchema(msg) => Status::invalid_argument(msg),

        DomainError::SchemaValidationFailed(device_id, reason) => {
            Status::invalid_argument(format!(
                "Schema validation failed for device {}: {}",
                device_id, reason
            ))
        }

        DomainError::EndDeviceDefinitionInUse(msg) => Status::failed_precondition(msg),

        DomainError::OrganizationNotFound(msg) => Status::not_found(msg),

        DomainError::OrganizationAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::OrganizationDeleted(msg) => Status::failed_precondition(msg),

        DomainError::WorkspaceNotFound(msg) => Status::not_found(msg),

        DomainError::WorkspaceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::WorkspaceDeleted(msg) => Status::failed_precondition(msg),

        DomainError::GatewayNotFound(msg) => Status::not_found(msg),

        DomainError::GatewayAlreadyExists(msg) => Status::already_exists(msg),

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
