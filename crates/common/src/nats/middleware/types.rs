use async_nats::HeaderMap;
use bytes::Bytes;

/// Request to publish a message to NATS
#[derive(Debug, Clone)]
pub struct PublishRequest {
    /// The subject to publish to
    pub subject: String,
    /// The message payload
    pub payload: Bytes,
    /// Optional headers (trace context will be injected here)
    pub headers: HeaderMap,
}

impl PublishRequest {
    pub fn new(subject: impl Into<String>, payload: impl Into<Bytes>) -> Self {
        Self {
            subject: subject.into(),
            payload: payload.into(),
            headers: HeaderMap::new(),
        }
    }

    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }
}

/// Response from a publish operation
#[derive(Debug)]
pub struct PublishResponse {
    /// The subject published to
    pub subject: String,
}
