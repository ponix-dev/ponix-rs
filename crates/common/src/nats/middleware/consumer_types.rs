use async_nats::HeaderMap;
use bytes::Bytes;

/// Request type for consuming a single NATS message through Tower.
///
/// This owns all the message data, allowing it to be passed through
/// Tower middleware layers without lifetime concerns.
#[derive(Debug, Clone)]
pub struct ConsumeRequest {
    /// The NATS subject the message was published to
    pub subject: String,
    /// The message payload
    pub payload: Bytes,
    /// Optional headers (used for trace context propagation)
    pub headers: Option<HeaderMap>,
}

impl ConsumeRequest {
    pub fn new(subject: String, payload: Bytes, headers: Option<HeaderMap>) -> Self {
        Self {
            subject,
            payload,
            headers,
        }
    }
}

/// Response type for message consumption.
///
/// Indicates whether the message should be acknowledged or rejected.
#[derive(Debug, Clone)]
pub enum ConsumeResponse {
    /// Message was processed successfully - acknowledge it
    Ack,
    /// Message processing failed - reject it for redelivery
    Nak(Option<String>),
}

impl ConsumeResponse {
    pub fn ack() -> Self {
        Self::Ack
    }

    pub fn nak(reason: impl Into<String>) -> Self {
        Self::Nak(Some(reason.into()))
    }

    pub fn nak_no_reason() -> Self {
        Self::Nak(None)
    }

    pub fn is_ack(&self) -> bool {
        matches!(self, Self::Ack)
    }

    pub fn is_nak(&self) -> bool {
        matches!(self, Self::Nak(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consume_request_new() {
        let req = ConsumeRequest::new("test.subject".to_string(), Bytes::from("payload"), None);

        assert_eq!(req.subject, "test.subject");
        assert_eq!(req.payload, Bytes::from("payload"));
        assert!(req.headers.is_none());
    }

    #[test]
    fn test_consume_response_ack() {
        let resp = ConsumeResponse::ack();
        assert!(resp.is_ack());
        assert!(!resp.is_nak());
    }

    #[test]
    fn test_consume_response_nak() {
        let resp = ConsumeResponse::nak("test error");
        assert!(!resp.is_ack());
        assert!(resp.is_nak());

        if let ConsumeResponse::Nak(Some(reason)) = resp {
            assert_eq!(reason, "test error");
        } else {
            panic!("Expected Nak with reason");
        }
    }

    #[test]
    fn test_consume_response_nak_no_reason() {
        let resp = ConsumeResponse::nak_no_reason();
        assert!(resp.is_nak());

        if let ConsumeResponse::Nak(reason) = resp {
            assert!(reason.is_none());
        } else {
            panic!("Expected Nak");
        }
    }
}
