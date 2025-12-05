use crate::domain::DomainError;
use tonic::Status;

/// Convert domain error to gRPC Status
pub fn domain_error_to_status(error: DomainError) -> Status {
    match error {
        DomainError::DeviceNotFound(msg) => Status::not_found(msg),

        DomainError::DeviceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidDeviceId(msg)
        | DomainError::InvalidOrganizationId(msg)
        | DomainError::InvalidDeviceName(msg)
        | DomainError::InvalidOrganizationName(msg) => Status::invalid_argument(msg),

        DomainError::PayloadConversionError(msg) => Status::invalid_argument(msg),

        DomainError::MissingCelExpression(msg) => Status::failed_precondition(msg),

        DomainError::OrganizationNotFound(msg) => Status::not_found(msg),

        DomainError::OrganizationAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::OrganizationDeleted(msg) => Status::failed_precondition(msg),

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

        DomainError::InvalidToken(msg) => Status::unauthenticated(format!("Invalid token: {}", msg)),

        DomainError::UserOrganizationAlreadyExists(user_id, org_id) => {
            Status::already_exists(format!("User {} already in organization {}", user_id, org_id))
        }

        DomainError::RepositoryError(err) => Status::internal(format!("Internal error: {}", err)),
    }
}
