use async_nats::HeaderMap;
use opentelemetry::{
    global,
    propagation::{Extractor, Injector},
    Context,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// W3C Trace Context header names
const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// Injector implementation for NATS HeaderMap
struct NatsHeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for NatsHeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value.as_str());
    }
}

/// Extractor implementation for NATS HeaderMap
struct NatsHeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for NatsHeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|v| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        vec![TRACEPARENT, TRACESTATE]
    }
}

/// Inject current span's trace context into NATS headers.
///
/// This should be called before publishing a message to NATS
/// to propagate the trace context to consumers.
///
/// Uses W3C Trace Context format (traceparent, tracestate headers).
pub fn inject_trace_context(headers: &mut HeaderMap) {
    global::get_text_map_propagator(|propagator| {
        let ctx = tracing::Span::current().context();
        propagator.inject_context(&ctx, &mut NatsHeaderInjector(headers));
    });
}

/// Extract trace context from NATS headers and return OpenTelemetry Context.
///
/// This should be called when consuming a message to extract
/// the trace context propagated from the publisher.
pub fn extract_trace_context(headers: &HeaderMap) -> Context {
    global::get_text_map_propagator(|propagator| propagator.extract(&NatsHeaderExtractor(headers)))
}

/// Set the parent context from NATS headers on the current span.
///
/// This connects the current span to the trace that published the message,
/// enabling distributed tracing across NATS message boundaries.
pub fn set_parent_from_headers(headers: &HeaderMap) {
    let ctx = extract_trace_context(headers);
    tracing::Span::current().set_parent(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_creates_headers() {
        let mut headers = HeaderMap::new();

        // Without OTEL initialized, this should not panic
        inject_trace_context(&mut headers);

        // Headers may or may not be populated depending on global propagator state
        // This test just verifies the function doesn't panic
    }

    #[test]
    fn test_extract_handles_empty_headers() {
        let headers = HeaderMap::new();

        // Should not panic with empty headers
        let _ctx = extract_trace_context(&headers);
    }

    #[test]
    fn test_set_parent_handles_empty_headers() {
        let headers = HeaderMap::new();

        // Should not panic with empty headers
        set_parent_from_headers(&headers);
    }

    #[test]
    fn test_manual_traceparent_extraction() {
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT, "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");

        // Verify the extractor can read the header
        let extractor = NatsHeaderExtractor(&headers);
        let value = extractor.get(TRACEPARENT);
        assert!(value.is_some());
        assert!(value.unwrap().starts_with("00-"));
    }
}
