use opentelemetry::logs::LogRecord as LogRecordTrait;
use opentelemetry::trace::TraceContextExt;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// A custom LogProcessor that adds trace_id and span_id as visible attributes
/// on every log record, in addition to the standard trace context metadata.
///
/// This makes trace correlation visible in log viewing tools like Loki/Grafana
/// without requiring derived field configuration.
#[derive(Debug)]
pub struct TraceContextLogProcessor<P: opentelemetry_sdk::logs::LogProcessor> {
    inner: P,
}

impl<P: opentelemetry_sdk::logs::LogProcessor> TraceContextLogProcessor<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P: opentelemetry_sdk::logs::LogProcessor> opentelemetry_sdk::logs::LogProcessor
    for TraceContextLogProcessor<P>
{
    fn emit(
        &self,
        record: &mut opentelemetry_sdk::logs::LogRecord,
        instrumentation: &opentelemetry::InstrumentationScope,
    ) {
        // Try to get trace context from the record first, then from current tracing span
        let trace_ids = record
            .trace_context
            .as_ref()
            .map(|tc| (tc.trace_id.to_string(), tc.span_id.to_string()))
            .or_else(|| {
                // Get from current tracing span's OTel context
                let current_span = Span::current();
                let otel_context = current_span.context();
                let otel_span = otel_context.span();
                let span_context = otel_span.span_context();
                if span_context.is_valid() {
                    Some((
                        span_context.trace_id().to_string(),
                        span_context.span_id().to_string(),
                    ))
                } else {
                    None
                }
            });

        if let Some((trace_id, span_id)) = trace_ids {
            LogRecordTrait::add_attribute(
                record,
                opentelemetry::Key::new("trace_id"),
                opentelemetry::logs::AnyValue::String(trace_id.into()),
            );
            LogRecordTrait::add_attribute(
                record,
                opentelemetry::Key::new("span_id"),
                opentelemetry::logs::AnyValue::String(span_id.into()),
            );
        }

        // Delegate to inner processor
        self.inner.emit(record, instrumentation);
    }

    fn force_flush(&self) -> opentelemetry_sdk::logs::LogResult<()> {
        self.inner.force_flush()
    }

    fn shutdown(&self) -> opentelemetry_sdk::logs::LogResult<()> {
        self.inner.shutdown()
    }

    fn set_resource(&self, resource: &opentelemetry_sdk::Resource) {
        self.inner.set_resource(resource);
    }
}
