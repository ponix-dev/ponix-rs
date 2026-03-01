use bytes::Bytes;
use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateDocumentRepoInput, DeleteDocumentRepoInput, Document, DocumentAssociationRepository,
    DocumentRepository, DomainError, DomainResult, GetDocumentRepoInput, GetOrganizationRepoInput,
    LinkDocumentInput, ListDocumentsByTargetInput, OrganizationRepository, UnlinkDocumentInput,
    UpdateDocumentRepoInput,
};
use common::nats::DocumentContentStore;
use garde::Validate;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, instrument, warn};

/// Service request for uploading a document through an association
#[derive(Debug, Clone, Validate)]
pub struct UploadDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub mime_type: String,
    #[garde(skip)]
    pub metadata: serde_json::Value,
    #[garde(skip)]
    pub content: Bytes,
}

/// Service request for updating a document
#[derive(Debug, Clone, Validate)]
pub struct UpdateDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(skip)]
    pub name: Option<String>,
    #[garde(skip)]
    pub mime_type: Option<String>,
    #[garde(skip)]
    pub metadata: Option<serde_json::Value>,
    #[garde(skip)]
    pub content: Option<Bytes>,
}

/// Service request for getting a document
#[derive(Debug, Clone, Validate)]
pub struct GetDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for deleting a document
#[derive(Debug, Clone, Validate)]
pub struct DeleteDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for downloading a document
#[derive(Debug, Clone, Validate)]
pub struct DownloadDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for linking a document to a target entity
#[derive(Debug, Clone, Validate)]
pub struct LinkDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub target_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(skip)]
    pub workspace_id: Option<String>,
}

/// Service request for unlinking a document from a target entity
#[derive(Debug, Clone, Validate)]
pub struct UnlinkDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub document_id: String,
    #[garde(length(min = 1))]
    pub target_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing documents by target entity
#[derive(Debug, Clone, Validate)]
pub struct ListDocumentsByTargetRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub target_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Domain service for document management business logic
pub struct DocumentService {
    document_repository: Arc<dyn DocumentRepository>,
    document_content_store: Arc<dyn DocumentContentStore>,
    document_association_repository: Arc<dyn DocumentAssociationRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl DocumentService {
    pub fn new(
        document_repository: Arc<dyn DocumentRepository>,
        document_content_store: Arc<dyn DocumentContentStore>,
        document_association_repository: Arc<dyn DocumentAssociationRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            document_repository,
            document_content_store,
            document_association_repository,
            organization_repository,
            authorization_provider,
        }
    }

    /// Upload a document: compute checksum, store content, persist metadata.
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, name = %request.name))]
    pub async fn upload_document(&self, request: UploadDocumentRequest) -> DomainResult<Document> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Create,
            )
            .await?;

        self.validate_organization(&request.organization_id).await?;

        let document_id = xid::new().to_string();
        let size_bytes = request.content.len() as i64;
        let checksum = compute_sha256(&request.content);
        let object_store_key = format!(
            "{}/{}/{}",
            request.organization_id, document_id, request.name
        );

        // Upload content to object store first
        self.document_content_store
            .upload(&object_store_key, request.content)
            .await
            .map_err(DomainError::RepositoryError)?;

        // Persist metadata to database
        let repo_input = CreateDocumentRepoInput {
            document_id: document_id.clone(),
            organization_id: request.organization_id.clone(),
            name: request.name,
            mime_type: request.mime_type,
            size_bytes,
            object_store_key: object_store_key.clone(),
            checksum,
            metadata: request.metadata,
        };

        let document = match self.document_repository.create_document(repo_input).await {
            Ok(doc) => doc,
            Err(e) => {
                // Best-effort cleanup of uploaded content
                warn!(
                    object_store_key = %object_store_key,
                    "Document metadata creation failed, cleaning up content"
                );
                let _ = self.document_content_store.delete(&object_store_key).await;
                return Err(e);
            }
        };

        debug!(document_id = %document.document_id, "Document uploaded successfully");
        Ok(document)
    }

    /// Update a document's metadata and optionally replace content.
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, organization_id = %request.organization_id))]
    pub async fn update_document(&self, request: UpdateDocumentRequest) -> DomainResult<Document> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Update,
            )
            .await?;

        // Get existing document to verify it exists and get the object store key
        let existing = self
            .get_document_internal(&request.document_id, &request.organization_id)
            .await?;

        // If new content is provided, upload it and compute new checksum/size
        let (size_bytes, checksum) = if let Some(ref content) = request.content {
            let new_checksum = compute_sha256(content);
            let new_size = content.len() as i64;

            // Upload new content to the same key (overwrites)
            self.document_content_store
                .upload(&existing.object_store_key, content.clone())
                .await
                .map_err(DomainError::RepositoryError)?;

            (Some(new_size), Some(new_checksum))
        } else {
            (None, None)
        };

        let repo_input = UpdateDocumentRepoInput {
            document_id: request.document_id,
            organization_id: request.organization_id,
            name: request.name,
            mime_type: request.mime_type,
            size_bytes,
            checksum,
            metadata: request.metadata,
        };

        let document = self.document_repository.update_document(repo_input).await?;
        debug!(document_id = %document.document_id, "Document updated successfully");
        Ok(document)
    }

    /// Get a document by ID
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, organization_id = %request.organization_id))]
    pub async fn get_document(&self, request: GetDocumentRequest) -> DomainResult<Document> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Read,
            )
            .await?;

        self.get_document_internal(&request.document_id, &request.organization_id)
            .await
    }

    /// Delete a document (soft delete metadata + best-effort content cleanup)
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, organization_id = %request.organization_id))]
    pub async fn delete_document(&self, request: DeleteDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Delete,
            )
            .await?;

        let doc = self
            .get_document_internal(&request.document_id, &request.organization_id)
            .await?;

        self.document_repository
            .delete_document(DeleteDocumentRepoInput {
                document_id: request.document_id,
                organization_id: request.organization_id,
            })
            .await?;

        // Best-effort content store cleanup
        if let Err(e) = self
            .document_content_store
            .delete(&doc.object_store_key)
            .await
        {
            warn!(
                object_store_key = %doc.object_store_key,
                error = %e,
                "Failed to delete document content from object store"
            );
        }

        debug!(document_id = %doc.document_id, "Document deleted successfully");
        Ok(())
    }

    /// Download a document's content
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, organization_id = %request.organization_id))]
    pub async fn download_document(
        &self,
        request: DownloadDocumentRequest,
    ) -> DomainResult<(Document, Bytes)> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Read,
            )
            .await?;

        let doc = self
            .get_document_internal(&request.document_id, &request.organization_id)
            .await?;

        let content = self
            .document_content_store
            .download(&doc.object_store_key)
            .await
            .map_err(DomainError::RepositoryError)?;

        Ok((doc, content))
    }

    // --- Association methods ---

    /// Link a document to a data stream
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn link_to_data_stream(&self, request: LinkDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Create,
            )
            .await?;

        self.document_association_repository
            .link_to_data_stream(LinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
                workspace_id: request.workspace_id,
            })
            .await
    }

    /// Unlink a document from a data stream
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn unlink_from_data_stream(
        &self,
        request: UnlinkDocumentRequest,
    ) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Delete,
            )
            .await?;

        self.document_association_repository
            .unlink_from_data_stream(UnlinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// List documents linked to a data stream
    #[instrument(skip(self, request), fields(user_id = %request.user_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn list_data_stream_documents(
        &self,
        request: ListDocumentsByTargetRequest,
    ) -> DomainResult<Vec<Document>> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Read,
            )
            .await?;

        self.document_association_repository
            .list_data_stream_documents(ListDocumentsByTargetInput {
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// Link a document to a definition
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn link_to_definition(&self, request: LinkDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Create,
            )
            .await?;

        self.document_association_repository
            .link_to_definition(LinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
                workspace_id: request.workspace_id,
            })
            .await
    }

    /// Unlink a document from a definition
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn unlink_from_definition(&self, request: UnlinkDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Delete,
            )
            .await?;

        self.document_association_repository
            .unlink_from_definition(UnlinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// List documents linked to a definition
    #[instrument(skip(self, request), fields(user_id = %request.user_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn list_definition_documents(
        &self,
        request: ListDocumentsByTargetRequest,
    ) -> DomainResult<Vec<Document>> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Read,
            )
            .await?;

        self.document_association_repository
            .list_definition_documents(ListDocumentsByTargetInput {
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// Link a document to a workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn link_to_workspace(&self, request: LinkDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Create,
            )
            .await?;

        self.document_association_repository
            .link_to_workspace(LinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
                workspace_id: request.workspace_id,
            })
            .await
    }

    /// Unlink a document from a workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, document_id = %request.document_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn unlink_from_workspace(&self, request: UnlinkDocumentRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Delete,
            )
            .await?;

        self.document_association_repository
            .unlink_from_workspace(UnlinkDocumentInput {
                document_id: request.document_id,
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// List documents linked to a workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, target_id = %request.target_id, organization_id = %request.organization_id))]
    pub async fn list_workspace_documents(
        &self,
        request: ListDocumentsByTargetRequest,
    ) -> DomainResult<Vec<Document>> {
        common::garde::validate_struct(&request)?;
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Document,
                Action::Read,
            )
            .await?;

        self.document_association_repository
            .list_workspace_documents(ListDocumentsByTargetInput {
                target_id: request.target_id,
                organization_id: request.organization_id,
            })
            .await
    }

    /// Internal helper to get a document or return DocumentNotFound
    async fn get_document_internal(
        &self,
        document_id: &str,
        organization_id: &str,
    ) -> DomainResult<Document> {
        let repo_input = GetDocumentRepoInput {
            document_id: document_id.to_string(),
            organization_id: organization_id.to_string(),
        };

        self.document_repository
            .get_document(repo_input)
            .await?
            .ok_or_else(|| {
                DomainError::DocumentNotFound(format!("Document not found: {}", document_id))
            })
    }

    /// Validate organization exists and is not deleted
    async fn validate_organization(&self, organization_id: &str) -> DomainResult<()> {
        let org_input = GetOrganizationRepoInput {
            organization_id: organization_id.to_string(),
        };

        match self
            .organization_repository
            .get_organization(org_input)
            .await?
        {
            Some(org) => {
                if org.deleted_at.is_some() {
                    return Err(DomainError::OrganizationDeleted(format!(
                        "Cannot operate on documents for deleted organization: {}",
                        organization_id
                    )));
                }
                Ok(())
            }
            None => Err(DomainError::OrganizationNotFound(format!(
                "Organization not found: {}",
                organization_id
            ))),
        }
    }
}

/// Compute SHA-256 checksum of content
fn compute_sha256(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        MockDocumentAssociationRepository, MockDocumentRepository, MockOrganizationRepository,
        Organization,
    };
    use common::nats::MockDocumentContentStore;

    const TEST_USER_ID: &str = "user-123";
    const TEST_ORG_ID: &str = "org-456";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn create_mock_org_repo_with_org() -> MockOrganizationRepository {
        let mut mock = MockOrganizationRepository::new();
        mock.expect_get_organization().returning(|input| {
            Ok(Some(Organization {
                id: input.organization_id.clone(),
                name: "Test Org".to_string(),
                deleted_at: None,
                created_at: None,
                updated_at: None,
            }))
        });
        mock
    }

    fn create_test_document(document_id: &str) -> Document {
        Document {
            document_id: document_id.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
            object_store_key: format!("{}/{}/test.pdf", TEST_ORG_ID, document_id),
            checksum: "abc123".to_string(),
            metadata: serde_json::json!({}),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_upload_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mut mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = create_mock_org_repo_with_org();

        mock_content_store.expect_upload().returning(|_, _| Ok(()));

        mock_doc_repo.expect_create_document().returning(|input| {
            Ok(Document {
                document_id: input.document_id,
                organization_id: input.organization_id,
                name: input.name,
                mime_type: input.mime_type,
                size_bytes: input.size_bytes,
                object_store_key: input.object_store_key,
                checksum: input.checksum,
                metadata: input.metadata,
                deleted_at: None,
                created_at: None,
                updated_at: None,
            })
        });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let content = Bytes::from("hello world");
        let request = UploadDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: serde_json::json!({"author": "Alice"}),
            content: content.clone(),
        };

        let result = service.upload_document(request).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert!(!doc.document_id.is_empty());
        assert_eq!(doc.organization_id, TEST_ORG_ID);
        assert_eq!(doc.name, "test.pdf");
        assert_eq!(doc.size_bytes, 11); // "hello world".len()
        assert_eq!(doc.checksum, compute_sha256(&content));
    }

    #[tokio::test]
    async fn test_upload_document_empty_name_fails() {
        let mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UploadDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: serde_json::json!({}),
            content: Bytes::from("data"),
        };

        let result = service.upload_document(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_upload_document_org_not_found() {
        let mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .returning(|_| Ok(None));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UploadDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent".to_string(),
            name: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: serde_json::json!({}),
            content: Bytes::from("data"),
        };

        let result = service.upload_document(request).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_upload_document_db_failure_cleans_up_content() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mut mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = create_mock_org_repo_with_org();

        mock_content_store.expect_upload().returning(|_, _| Ok(()));

        mock_content_store.expect_delete().returning(|_| Ok(()));

        mock_doc_repo.expect_create_document().returning(|_| {
            Err(DomainError::RepositoryError(anyhow::anyhow!(
                "database error"
            )))
        });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UploadDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: serde_json::json!({}),
            content: Bytes::from("data"),
        };

        let result = service.upload_document(request).await;
        assert!(matches!(result, Err(DomainError::RepositoryError(_))));
    }

    #[tokio::test]
    async fn test_get_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let expected_doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(expected_doc.clone())));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = GetDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.get_document(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().document_id, "doc-123");
    }

    #[tokio::test]
    async fn test_get_document_not_found() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        mock_doc_repo.expect_get_document().returning(|_| Ok(None));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = GetDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "nonexistent".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.get_document(request).await;
        assert!(matches!(result, Err(DomainError::DocumentNotFound(_))));
    }

    #[tokio::test]
    async fn test_delete_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mut mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(doc.clone())));

        mock_doc_repo.expect_delete_document().returning(|_| Ok(()));

        mock_content_store.expect_delete().returning(|_| Ok(()));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = DeleteDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.delete_document(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_download_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mut mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(doc.clone())));

        mock_content_store
            .expect_download()
            .returning(|_| Ok(Bytes::from("file content")));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = DownloadDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.download_document(request).await;
        assert!(result.is_ok());
        let (doc, content) = result.unwrap();
        assert_eq!(doc.document_id, "doc-123");
        assert_eq!(content, Bytes::from("file content"));
    }

    #[tokio::test]
    async fn test_update_document_metadata_only() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let existing_doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(existing_doc.clone())));

        mock_doc_repo.expect_update_document().returning(|input| {
            Ok(Document {
                document_id: input.document_id,
                organization_id: input.organization_id,
                name: input.name.unwrap_or_else(|| "test.pdf".to_string()),
                mime_type: "application/pdf".to_string(),
                size_bytes: 1024,
                object_store_key: "key".to_string(),
                checksum: "abc123".to_string(),
                metadata: serde_json::json!({}),
                deleted_at: None,
                created_at: None,
                updated_at: None,
            })
        });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UpdateDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: Some("renamed.pdf".to_string()),
            mime_type: None,
            metadata: None,
            content: None,
        };

        let result = service.update_document(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "renamed.pdf");
    }

    #[tokio::test]
    async fn test_permission_denied() {
        let mock_doc_repo = MockDocumentRepository::new();
        let mock_content_store = MockDocumentContentStore::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let mut mock_auth = MockAuthorizationProvider::new();
        mock_auth
            .expect_require_permission()
            .returning(|_, _, _, _| {
                Box::pin(async {
                    Err(DomainError::PermissionDenied(
                        "User does not have permission".to_string(),
                    ))
                })
            });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(mock_content_store),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            Arc::new(mock_auth),
        );

        let request = GetDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.get_document(request).await;
        assert!(matches!(result, Err(DomainError::PermissionDenied(_))));
    }
}
