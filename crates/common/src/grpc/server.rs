//! Reusable gRPC server with optional gRPC-Web and CORS support.
//!
//! # Example
//!
//! ```ignore
//! use common::grpc::{GrpcServerConfig, CorsConfig, run_grpc_server};
//! use tonic::service::Routes;
//!
//! let config = GrpcServerConfig {
//!     host: "0.0.0.0".to_string(),
//!     port: 50051,
//!     enable_grpc_web: true,
//!     cors_config: Some(CorsConfig::allow_all()),
//!     ..Default::default()
//! };
//!
//! // Build routes with all services
//! let routes = Routes::builder()
//!     .add_service(EndDeviceServiceServer::new(device_handler))
//!     .add_service(OrganizationServiceServer::new(org_handler))
//!     .routes();
//!
//! run_grpc_server(
//!     config,
//!     routes,
//!     &[ponix_proto_prost::end_device::v1::FILE_DESCRIPTOR_SET],
//!     cancellation_token,
//! ).await?;
//! ```

use std::net::SocketAddr;
use std::time::Duration;

use http::{header::HeaderName, Method};
use tokio_util::sync::CancellationToken;
use tonic::service::Routes;
use tonic::transport::Server;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::debug;

use super::{GrpcLoggingConfig, GrpcLoggingLayer, GrpcTracingConfig, GrpcTracingLayer};

/// CORS configuration for the gRPC server.
#[derive(Debug, Clone)]
pub struct CorsConfig {
    /// Allowed origins. Use `vec!["*".to_string()]` to allow all origins.
    pub allowed_origins: Vec<String>,
    /// Max age for CORS preflight cache in seconds.
    pub max_age_secs: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            max_age_secs: 3600,
        }
    }
}

impl CorsConfig {
    /// Create a CORS config that allows all origins.
    pub fn allow_all() -> Self {
        Self::default()
    }

    /// Create a CORS config with specific allowed origins.
    pub fn with_origins(origins: Vec<String>) -> Self {
        Self {
            allowed_origins: origins,
            max_age_secs: 3600,
        }
    }

    /// Parse comma-separated origins string.
    pub fn from_comma_separated(origins: &str) -> Self {
        let allowed_origins: Vec<String> = origins
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            allowed_origins: if allowed_origins.is_empty() {
                vec!["*".to_string()]
            } else {
                allowed_origins
            },
            max_age_secs: 3600,
        }
    }
}

/// Configuration for the gRPC server.
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    /// Server host address.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Logging middleware configuration.
    pub logging_config: GrpcLoggingConfig,
    /// Tracing middleware configuration.
    pub tracing_config: GrpcTracingConfig,
    /// Enable gRPC-Web support (HTTP/1.1).
    pub enable_grpc_web: bool,
    /// CORS configuration. Only used when gRPC-Web is enabled.
    pub cors_config: Option<CorsConfig>,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 50051,
            logging_config: GrpcLoggingConfig::default(),
            tracing_config: GrpcTracingConfig::default(),
            enable_grpc_web: false,
            cors_config: None,
        }
    }
}

impl GrpcServerConfig {
    /// Create a config with gRPC-Web and CORS enabled.
    pub fn with_grpc_web(mut self, cors_config: CorsConfig) -> Self {
        self.enable_grpc_web = true;
        self.cors_config = Some(cors_config);
        self
    }
}

/// Build a CORS layer from configuration.
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    let allow_origin = if config.allowed_origins.len() == 1 && config.allowed_origins[0] == "*" {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            config
                .allowed_origins
                .iter()
                .filter_map(|origin| origin.parse().ok()),
        )
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("x-grpc-web"),
            HeaderName::from_static("x-user-agent"),
            HeaderName::from_static("grpc-timeout"),
            HeaderName::from_static("authorization"),
        ])
        .expose_headers([
            HeaderName::from_static("grpc-status"),
            HeaderName::from_static("grpc-message"),
            HeaderName::from_static("grpc-status-details-bin"),
        ])
        .max_age(Duration::from_secs(config.max_age_secs))
}

/// Build the reflection service from file descriptor sets.
fn build_reflection_service(
    descriptors: &[&'static [u8]],
) -> tonic_reflection::server::ServerReflectionServer<
    impl tonic_reflection::server::ServerReflection,
> {
    let mut builder = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(protoc_wkt::google::protobuf::FILE_DESCRIPTOR_SET);

    for descriptor in descriptors {
        builder = builder.register_encoded_file_descriptor_set(descriptor);
    }

    builder
        .build_v1()
        .expect("Failed to build reflection service")
}

/// Run a gRPC server with the provided routes and configuration.
///
/// This function applies layers based on the configuration:
/// - Always applies: logging, tracing
/// - When `enable_grpc_web` is true: CORS, gRPC-Web layers
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `routes` - Pre-built routes containing all gRPC services
/// * `reflection_descriptors` - File descriptor sets for reflection service
/// * `cancellation_token` - Token for graceful shutdown
///
/// # Example
///
/// ```ignore
/// let routes = Routes::builder()
///     .add_service(MyServiceServer::new(handler))
///     .routes();
///
/// run_grpc_server(
///     config,
///     routes,
///     &[FILE_DESCRIPTOR_SET],
///     token,
/// ).await?;
/// ```
pub async fn run_grpc_server(
    config: GrpcServerConfig,
    routes: Routes,
    reflection_descriptors: &[&'static [u8]],
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid server address");

    let grpc_web_status = if config.enable_grpc_web {
        "enabled"
    } else {
        "disabled"
    };

    debug!(
        address = %addr,
        grpc_web = %grpc_web_status,
        "Starting gRPC server"
    );

    // Build layers
    let logging_layer = GrpcLoggingLayer::new(config.logging_config.clone());
    let tracing_layer = GrpcTracingLayer::new(config.tracing_config.clone());

    // Build reflection service
    let reflection_service = build_reflection_service(reflection_descriptors);

    // Build and serve based on gRPC-Web configuration
    // Note: add_routes must be called before add_service, as add_service transitions to Router
    if config.enable_grpc_web {
        let cors_layer = config
            .cors_config
            .as_ref()
            .map(build_cors_layer)
            .unwrap_or_else(|| build_cors_layer(&CorsConfig::default()));
        let grpc_web_layer = GrpcWebLayer::new();

        let router = Server::builder()
            .accept_http1(true)
            .layer(tracing_layer)
            .layer(logging_layer)
            .layer(cors_layer)
            .layer(grpc_web_layer)
            .add_routes(routes)
            .add_service(reflection_service);

        let serve = router.serve_with_shutdown(addr, async move {
            cancellation_token.cancelled().await;
            debug!("gRPC server shutdown signal received");
        });

        match serve.await {
            Ok(_) => {
                debug!("gRPC server stopped gracefully");
                Ok(())
            }
            Err(e) => {
                tracing::error!("gRPC server error: {}", e);
                Err(e.into())
            }
        }
    } else {
        let router = Server::builder()
            .layer(tracing_layer)
            .layer(logging_layer)
            .add_routes(routes)
            .add_service(reflection_service);

        let serve = router.serve_with_shutdown(addr, async move {
            cancellation_token.cancelled().await;
            debug!("gRPC server shutdown signal received");
        });

        match serve.await {
            Ok(_) => {
                debug!("gRPC server stopped gracefully");
                Ok(())
            }
            Err(e) => {
                tracing::error!("gRPC server error: {}", e);
                Err(e.into())
            }
        }
    }
}
