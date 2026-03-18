use std::sync::Arc;
use std::task::{Context, Poll};

use common::nats::{ConsumeRequest, ConsumeResponse};
use futures::future::BoxFuture;
use tower::Service;
use tracing::{debug, error};

use crate::domain::SnapshotterService;

/// Tower service that processes individual NATS document sync messages.
/// Extracts the document ID from the subject and applies the Yrs update.
#[derive(Clone)]
pub struct DocumentUpdateService {
    snapshotter_service: Arc<SnapshotterService>,
}

impl DocumentUpdateService {
    pub fn new(snapshotter_service: Arc<SnapshotterService>) -> Self {
        Self {
            snapshotter_service,
        }
    }
}

impl Service<ConsumeRequest> for DocumentUpdateService {
    type Response = ConsumeResponse;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ConsumeRequest) -> Self::Future {
        let service = Arc::clone(&self.snapshotter_service);
        let subject = req.subject.clone();
        let payload = req.payload.clone();

        Box::pin(async move {
            let document_id = match parse_document_id_from_subject(&subject) {
                Some(id) => id,
                None => {
                    error!(subject = %subject, "invalid document sync subject format");
                    return Ok(ConsumeResponse::Nak(Some(format!(
                        "invalid subject: {}",
                        subject
                    ))));
                }
            };

            debug!(document_id = %document_id, "applying document update from NATS");

            match service.apply_update(document_id, &payload).await {
                Ok(()) => Ok(ConsumeResponse::Ack),
                Err(e) => {
                    error!(
                        document_id = %document_id,
                        error = %e,
                        "failed to apply document update"
                    );
                    Ok(ConsumeResponse::Nak(Some(format!("apply error: {}", e))))
                }
            }
        })
    }
}

/// Extract document ID from NATS subject like "document_sync.{document_id}"
fn parse_document_id_from_subject(subject: &str) -> Option<&str> {
    let parts: Vec<&str> = subject.splitn(2, '.').collect();
    if parts.len() == 2 && !parts[1].is_empty() {
        Some(parts[1])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_subject() {
        assert_eq!(
            parse_document_id_from_subject("document_sync.doc-123"),
            Some("doc-123")
        );
    }

    #[test]
    fn test_parse_subject_with_no_dot() {
        assert_eq!(parse_document_id_from_subject("document_sync"), None);
    }

    #[test]
    fn test_parse_subject_with_trailing_dot() {
        assert_eq!(parse_document_id_from_subject("document_sync."), None);
    }

    #[test]
    fn test_parse_subject_with_complex_id() {
        assert_eq!(
            parse_document_id_from_subject("document_sync.abc-def-123"),
            Some("abc-def-123")
        );
    }
}
