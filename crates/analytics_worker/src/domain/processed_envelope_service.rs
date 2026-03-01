use common::domain::{DomainResult, ProcessedEnvelopeRepository, StoreEnvelopesInput};
use std::sync::Arc;
use tracing::{debug, instrument};

/// Domain service for processed envelope operations
pub struct ProcessedEnvelopeService {
    repository: Arc<dyn ProcessedEnvelopeRepository>,
}

impl ProcessedEnvelopeService {
    pub fn new(repository: Arc<dyn ProcessedEnvelopeRepository>) -> Self {
        Self { repository }
    }

    /// Store a batch of processed envelopes
    /// Currently a pass-through to repository, but provides extension point for future validation
    #[instrument(skip(self), fields(envelope_count = input.envelopes.len()))]
    pub async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()> {
        debug!(
            envelope_count = input.envelopes.len(),
            "storing batch of envelopes"
        );

        self.repository.store_batch(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{DomainError, MockProcessedEnvelopeRepository, ProcessedEnvelope};

    // ProcessedEnvelope already imported via use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_store_batch_success() {
        let mut mock_repo = MockProcessedEnvelopeRepository::new();

        // Setup mock expectation
        mock_repo
            .expect_store_batch()
            .withf(|input: &StoreEnvelopesInput| input.envelopes.len() == 2)
            .times(1)
            .return_once(|_| Ok(()));

        let service = ProcessedEnvelopeService::new(Arc::new(mock_repo));

        // Create test envelopes
        let envelopes = vec![
            ProcessedEnvelope {
                organization_id: "org-123".to_string(),
                data_stream_id: "ds-456".to_string(),
                received_at: Utc::now(),
                processed_at: Utc::now(),
                data: serde_json::Map::new(),
            },
            ProcessedEnvelope {
                organization_id: "org-123".to_string(),
                data_stream_id: "ds-789".to_string(),
                received_at: Utc::now(),
                processed_at: Utc::now(),
                data: serde_json::Map::new(),
            },
        ];

        let input = StoreEnvelopesInput { envelopes };

        // Call service
        let result = service.store_batch(input).await;

        // Verify
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_store_batch_repository_error() {
        let mut mock_repo = MockProcessedEnvelopeRepository::new();

        // Setup mock to return error
        mock_repo.expect_store_batch().times(1).return_once(|_| {
            Err(DomainError::RepositoryError(anyhow::anyhow!(
                "Database connection failed"
            )))
        });

        let service = ProcessedEnvelopeService::new(Arc::new(mock_repo));

        let input = StoreEnvelopesInput { envelopes: vec![] };

        // Call service
        let result = service.store_batch(input).await;

        // Verify error propagation
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::RepositoryError(_)
        ));
    }
}
