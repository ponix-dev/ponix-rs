use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use http::{Request, Response};
use opentelemetry::{global, propagation::Extractor, trace::TraceContextExt as _};
use tower::{Layer, Service};
use tracing::{field, info_span, Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Configuration for gRPC tracing middleware
#[derive(Clone, Debug)]
pub struct GrpcTracingConfig {
    /// List of path prefixes to skip tracing (e.g., "/grpc.reflection.")
    pub ignored_paths: Vec<String>,
}

impl Default for GrpcTracingConfig {
    fn default() -> Self {
        Self {
            ignored_paths: vec!["/grpc.reflection.".to_string()],
        }
    }
}

impl GrpcTracingConfig {
    pub fn new(ignored_paths: Vec<String>) -> Self {
        Self { ignored_paths }
    }

    fn should_ignore(&self, path: &str) -> bool {
        self.ignored_paths
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }
}

/// Tower layer for OpenTelemetry tracing of gRPC requests
#[derive(Clone)]
pub struct GrpcTracingLayer {
    config: GrpcTracingConfig,
}

impl GrpcTracingLayer {
    pub fn new(config: GrpcTracingConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self {
            config: GrpcTracingConfig::default(),
        }
    }
}

impl<S> Layer<S> for GrpcTracingLayer {
    type Service = GrpcTracingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        GrpcTracingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that adds OpenTelemetry tracing to gRPC requests
#[derive(Clone)]
pub struct GrpcTracingService<S> {
    inner: S,
    config: GrpcTracingConfig,
}

/// Extractor for HTTP headers (gRPC metadata)
struct HttpHeaderExtractor<'a>(&'a http::HeaderMap);

impl Extractor for HttpHeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Extract method name suffix from gRPC path
/// e.g., "/ponix.end_device.v1.EndDeviceService/CreateEndDevice" -> "CreateEndDevice"
fn extract_method_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Extract service name from gRPC path
/// e.g., "/ponix.end_device.v1.EndDeviceService/CreateEndDevice" -> "EndDeviceService"
fn extract_service_name(path: &str) -> &str {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() >= 2 {
        // Get the service part (e.g., "ponix.end_device.v1.EndDeviceService")
        // and extract just the service name
        parts[0].rsplit('.').next().unwrap_or(parts[0])
    } else {
        path
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for GrpcTracingService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Error: std::fmt::Display,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let path = req.uri().path().to_string();

        // Skip tracing for ignored paths
        if self.config.should_ignore(&path) {
            return Box::pin(self.inner.call(req));
        }

        // Extract trace context from incoming headers
        let parent_context = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HttpHeaderExtractor(req.headers()))
        });

        // Extract method and service names for span attributes
        let method_name = extract_method_name(&path).to_string();
        let service_name = extract_service_name(&path).to_string();

        // Create span with OpenTelemetry semantic conventions for gRPC
        let span = info_span!(
            target: "grpc",
            "grpc_request",
            otel.name = %method_name,
            rpc.system = "grpc",
            rpc.service = %service_name,
            rpc.method = %method_name,
            rpc.grpc.status_code = field::Empty,
            trace_id = field::Empty,
            span_id = field::Empty,
        );

        // Set parent context from extracted headers
        span.set_parent(parent_context);

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                // Record trace_id and span_id now that we're inside the span
                // The OTel layer will have assigned IDs at this point
                let current_span = Span::current();
                let otel_context = current_span.context();
                let otel_span = otel_context.span();
                let span_context = otel_span.span_context();
                if span_context.is_valid() {
                    current_span.record("trace_id", span_context.trace_id().to_string());
                    current_span.record("span_id", span_context.span_id().to_string());
                }

                let result = inner.call(req).await;

                // Record gRPC status code on the span
                match &result {
                    Ok(response) => {
                        let grpc_status = response
                            .headers()
                            .get("grpc-status")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<i32>().ok())
                            .unwrap_or(0);

                        Span::current().record("rpc.grpc.status_code", grpc_status);

                        // Set span status based on gRPC status
                        if grpc_status != 0 {
                            // Non-zero status indicates an error
                            Span::current().record("otel.status_code", "ERROR");
                        }
                    }
                    Err(_) => {
                        Span::current().record("rpc.grpc.status_code", 2); // UNKNOWN
                        Span::current().record("otel.status_code", "ERROR");
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_method_name() {
        assert_eq!(
            extract_method_name("/ponix.end_device.v1.EndDeviceService/CreateEndDevice"),
            "CreateEndDevice"
        );
        assert_eq!(
            extract_method_name("/ponix.organization.v1.OrganizationService/GetOrganization"),
            "GetOrganization"
        );
        assert_eq!(extract_method_name("/Service/Method"), "Method");
        assert_eq!(extract_method_name("Method"), "Method");
    }

    #[test]
    fn test_extract_service_name() {
        assert_eq!(
            extract_service_name("/ponix.end_device.v1.EndDeviceService/CreateEndDevice"),
            "EndDeviceService"
        );
        assert_eq!(
            extract_service_name("/ponix.organization.v1.OrganizationService/GetOrganization"),
            "OrganizationService"
        );
    }

    #[test]
    fn test_config_should_ignore() {
        let config = GrpcTracingConfig::default();
        assert!(config.should_ignore("/grpc.reflection.v1/ServerReflectionInfo"));
        assert!(!config.should_ignore("/ponix.end_device.v1.EndDeviceService/CreateEndDevice"));
    }
}
