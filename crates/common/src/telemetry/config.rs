use opentelemetry_sdk::{logs::LoggerProvider, trace::TracerProvider as SdkTracerProvider};

/// Configuration for telemetry initialization
#[derive(Clone, Debug)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub otel_endpoint: String,
    pub otel_enabled: bool,
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "unknown-service".to_string(),
            otel_endpoint: "http://localhost:4317".to_string(),
            otel_enabled: false,
            log_level: "info".to_string(),
        }
    }
}

/// Providers returned from telemetry initialization for proper shutdown
pub struct TelemetryProviders {
    pub tracer_provider: SdkTracerProvider,
    pub logger_provider: LoggerProvider,
}
