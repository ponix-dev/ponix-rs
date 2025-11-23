use ponix_domain::DomainError;
use tonic::Status;

/// Convert domain error to gRPC Status
pub fn domain_error_to_status(error: DomainError) -> Status {
    match error {
        DomainError::DeviceNotFound(msg) => Status::not_found(msg),

        DomainError::DeviceAlreadyExists(msg) => Status::already_exists(msg),

        DomainError::InvalidDeviceId(msg)
        | DomainError::InvalidOrganizationId(msg)
        | DomainError::InvalidDeviceName(msg) => Status::invalid_argument(msg),

        DomainError::PayloadConversionError(msg) => Status::invalid_argument(msg),

        DomainError::MissingCelExpression(msg) => Status::failed_precondition(msg),

        DomainError::RepositoryError(err) => Status::internal(format!("Internal error: {}", err)),
    }
}
