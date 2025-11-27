use tonic::{Response, Status};
use tracing::Span;

/// Record gRPC status code on the current span.
///
/// This follows OpenTelemetry semantic conventions for gRPC:
/// - `rpc.grpc.status_code`: The numeric status code
///
/// Call this at the end of a gRPC handler to record the response status.
pub fn record_grpc_status<T>(result: &Result<Response<T>, Status>) {
    let code = match result {
        Ok(_) => 0, // OK
        Err(status) => status.code() as i32,
    };

    Span::current().record("rpc.grpc.status_code", code);
}

/// Extension trait to record gRPC status on Result and return it unchanged.
///
/// This allows chaining: `result.record_status()`
pub trait RecordGrpcStatus<T> {
    fn record_status(self) -> Self;
}

impl<T> RecordGrpcStatus<T> for Result<Response<T>, Status> {
    fn record_status(self) -> Self {
        record_grpc_status(&self);
        self
    }
}
