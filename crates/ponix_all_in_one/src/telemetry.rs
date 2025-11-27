use anyhow::Result;
use opentelemetry::{trace::TracerProvider, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, WithExportConfig};
use opentelemetry_sdk::{
    logs::LoggerProvider,
    propagation::TraceContextPropagator,
    runtime,
    trace::{RandomIdGenerator, Sampler, TracerProvider as SdkTracerProvider},
    Resource,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Configuration for telemetry initialization
pub struct TelemetryConfig {
    pub service_name: String,
    pub otel_endpoint: String,
    pub otel_enabled: bool,
    pub log_level: String,
}

/// Providers returned from telemetry initialization for proper shutdown
pub struct TelemetryProviders {
    pub tracer_provider: SdkTracerProvider,
    pub logger_provider: LoggerProvider,
}

/// Initialize telemetry with OpenTelemetry support
///
/// When OTEL is enabled:
/// - Sets up OTLP exporter for traces
/// - Sets up OTLP exporter for logs (sends to Loki via OTLP)
/// - Bridges tracing spans to OpenTelemetry
/// - Configures W3C Trace Context propagation
///
/// When OTEL is disabled:
/// - Falls back to JSON logging only
pub fn init_telemetry(config: &TelemetryConfig) -> Result<Option<TelemetryProviders>> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_span_list(true)
        .with_current_span(true);

    if config.otel_enabled {
        // Set global propagator for W3C Trace Context
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        // Create resource with service name (shared between traces and logs)
        let resource = Resource::new(vec![KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            config.service_name.clone(),
        )]);

        // Initialize OTLP trace exporter
        let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.otel_endpoint)
            .build()?;

        // Create tracer provider
        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(trace_exporter, runtime::Tokio)
            .with_sampler(Sampler::AlwaysOn)
            .with_id_generator(RandomIdGenerator::default())
            .with_resource(resource.clone())
            .build();

        // Initialize OTLP log exporter
        let log_exporter = LogExporter::builder()
            .with_tonic()
            .with_endpoint(&config.otel_endpoint)
            .build()?;

        // Create logger provider
        let logger_provider = LoggerProvider::builder()
            .with_batch_exporter(log_exporter, runtime::Tokio)
            .with_resource(resource)
            .build();

        // Create OpenTelemetry tracing layer for spans
        let tracer = tracer_provider.tracer("ponix");
        let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        // Create OpenTelemetry logging layer - bridges tracing events to OTLP logs
        let otel_log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_trace_layer)
            .with(otel_log_layer)
            .init();

        Ok(Some(TelemetryProviders {
            tracer_provider,
            logger_provider,
        }))
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(None)
    }
}

/// Shutdown telemetry and flush any pending traces and logs
pub fn shutdown_telemetry(providers: Option<TelemetryProviders>) {
    if let Some(providers) = providers {
        if let Err(e) = providers.tracer_provider.shutdown() {
            eprintln!("Error shutting down tracer provider: {:?}", e);
        }
        if let Err(e) = providers.logger_provider.shutdown() {
            eprintln!("Error shutting down logger provider: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_creation() {
        let config = TelemetryConfig {
            service_name: "test-service".to_string(),
            otel_endpoint: "http://localhost:4317".to_string(),
            otel_enabled: false,
            log_level: "info".to_string(),
        };

        assert_eq!(config.service_name, "test-service");
        assert!(!config.otel_enabled);
    }
}
