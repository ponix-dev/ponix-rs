use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};
use tracing::{error, info, Instrument, Span};

/// Configuration for gRPC request logging
#[derive(Clone, Debug)]
pub struct GrpcLoggingConfig {
    /// List of path prefixes to ignore (e.g., "/grpc.reflection.")
    pub ignored_paths: Vec<String>,
}

impl Default for GrpcLoggingConfig {
    fn default() -> Self {
        Self {
            ignored_paths: vec!["/grpc.reflection.".to_string()],
        }
    }
}

impl GrpcLoggingConfig {
    /// Create a new config with custom ignored paths
    pub fn new(ignored_paths: Vec<String>) -> Self {
        Self { ignored_paths }
    }

    /// Check if a path should be ignored
    fn should_ignore(&self, path: &str) -> bool {
        self.ignored_paths
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }
}

/// Tower layer for logging gRPC requests
#[derive(Clone)]
pub struct GrpcLoggingLayer {
    config: GrpcLoggingConfig,
}

impl GrpcLoggingLayer {
    /// Create a new logging layer with the given configuration
    pub fn new(config: GrpcLoggingConfig) -> Self {
        Self { config }
    }

    /// Create a logging layer with default configuration
    pub fn with_defaults() -> Self {
        Self {
            config: GrpcLoggingConfig::default(),
        }
    }
}

impl<S> Layer<S> for GrpcLoggingLayer {
    type Service = GrpcLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        GrpcLoggingService {
            inner: service,
            config: self.config.clone(),
        }
    }
}

/// Service that logs gRPC requests
#[derive(Clone)]
pub struct GrpcLoggingService<S> {
    inner: S,
    config: GrpcLoggingConfig,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for GrpcLoggingService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
    S::Error: std::fmt::Display,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let path = req.uri().path().to_string();
        let should_ignore = self.config.should_ignore(&path);
        let start = Instant::now();
        let future = self.inner.call(req);

        // Capture the current span so logs are correlated with the parent trace
        let span = Span::current();

        Box::pin(
            async move {
                let result = future.await;

                // Skip logging for ignored paths
                if !should_ignore {
                    let duration = start.elapsed();

                    match &result {
                        Ok(response) => {
                            let status = response.status();
                            let grpc_status = response
                                .headers()
                                .get("grpc-status")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("0");

                            info!(
                                path = %path,
                                http_status = %status.as_u16(),
                                grpc_status = %grpc_status,
                                duration_ms = %duration.as_millis(),
                                "{} - {}ms - gRPC status: {}",
                                path,
                                duration.as_millis(),
                                grpc_status
                            );
                        }
                        Err(e) => {
                            error!(
                                path = %path,
                                duration_ms = %duration.as_millis(),
                                error = %e,
                                "{} - {}ms - ERROR: {}",
                                path,
                                duration.as_millis(),
                                e
                            );
                        }
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}
