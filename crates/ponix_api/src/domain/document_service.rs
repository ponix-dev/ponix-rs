use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateDocumentRepoInputWithId, DeleteDocumentRepoInput, Document,
    DocumentAssociationRepository, DocumentRepository, DomainError, DomainResult,
    GetDocumentRepoInput, GetOrganizationRepoInput, LinkDocumentInput, ListDocumentsByTargetInput,
    OrganizationRepository, UnlinkDocumentInput, UpdateDocumentRepoInput,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a document
#[derive(Debug, Clone, Validate)]
pub struct CreateDocumentRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(skip)]
    pub metadata: serde_json::Value,
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
    pub metadata: Option<serde_json::Value>,
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
    document_association_repository: Arc<dyn DocumentAssociationRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl DocumentService {
    pub fn new(
        document_repository: Arc<dyn DocumentRepository>,
        document_association_repository: Arc<dyn DocumentAssociationRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            document_repository,
            document_association_repository,
            organization_repository,
            authorization_provider,
        }
    }

    /// Create a document with empty Yrs state.
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, name = %request.name))]
    pub async fn create_document(&self, request: CreateDocumentRequest) -> DomainResult<Document> {
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

        let repo_input = CreateDocumentRepoInputWithId {
            document_id,
            organization_id: request.organization_id,
            name: request.name,
            yrs_state: vec![],
            yrs_state_vector: vec![],
            content_text: String::new(),
            content_html: String::new(),
            metadata: request.metadata,
        };

        let document = self.document_repository.create_document(repo_input).await?;

        debug!(document_id = %document.document_id, "Document created successfully");
        Ok(document)
    }

    /// Update a document's name and/or metadata.
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

        // Verify document exists
        self.get_document_internal(&request.document_id, &request.organization_id)
            .await?;

        let repo_input = UpdateDocumentRepoInput {
            document_id: request.document_id,
            organization_id: request.organization_id,
            name: request.name,
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

    /// Delete a document (soft delete)
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

        self.document_repository
            .delete_document(DeleteDocumentRepoInput {
                document_id: request.document_id.clone(),
                organization_id: request.organization_id,
            })
            .await?;

        debug!(document_id = %request.document_id, "Document deleted successfully");
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        MockDocumentAssociationRepository, MockDocumentRepository, MockOrganizationRepository,
        Organization,
    };

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
            name: "test doc".to_string(),
            yrs_state: vec![],
            yrs_state_vector: vec![],
            content_text: String::new(),
            content_html: String::new(),
            metadata: serde_json::json!({}),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_create_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_org_repo = create_mock_org_repo_with_org();

        mock_doc_repo.expect_create_document().returning(|input| {
            Ok(Document {
                document_id: input.document_id,
                organization_id: input.organization_id,
                name: input.name,
                yrs_state: input.yrs_state,
                yrs_state_vector: input.yrs_state_vector,
                content_text: input.content_text,
                content_html: input.content_html,
                metadata: input.metadata,
                deleted_at: None,
                created_at: None,
                updated_at: None,
            })
        });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "test doc".to_string(),
            metadata: serde_json::json!({"author": "Alice"}),
        };

        let result = service.create_document(request).await;
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert!(!doc.document_id.is_empty());
        assert_eq!(doc.organization_id, TEST_ORG_ID);
        assert_eq!(doc.name, "test doc");
        assert!(doc.yrs_state.is_empty());
        assert!(doc.yrs_state_vector.is_empty());
    }

    #[tokio::test]
    async fn test_create_document_empty_name_fails() {
        let mock_doc_repo = MockDocumentRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "".to_string(),
            metadata: serde_json::json!({}),
        };

        let result = service.create_document(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_create_document_org_not_found() {
        let mock_doc_repo = MockDocumentRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .returning(|_| Ok(None));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent".to_string(),
            name: "test doc".to_string(),
            metadata: serde_json::json!({}),
        };

        let result = service.create_document(request).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_document_success() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let expected_doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(expected_doc.clone())));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
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
        let mock_org_repo = MockOrganizationRepository::new();

        mock_doc_repo.expect_get_document().returning(|_| Ok(None));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
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
        let mock_org_repo = MockOrganizationRepository::new();

        mock_doc_repo.expect_delete_document().returning(|_| Ok(()));

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
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
    async fn test_update_document_metadata_only() {
        let mut mock_doc_repo = MockDocumentRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let existing_doc = create_test_document("doc-123");
        mock_doc_repo
            .expect_get_document()
            .returning(move |_| Ok(Some(existing_doc.clone())));

        mock_doc_repo.expect_update_document().returning(|input| {
            Ok(Document {
                document_id: input.document_id,
                organization_id: input.organization_id,
                name: input.name.unwrap_or_else(|| "test doc".to_string()),
                yrs_state: vec![],
                yrs_state_vector: vec![],
                content_text: String::new(),
                content_html: String::new(),
                metadata: serde_json::json!({}),
                deleted_at: None,
                created_at: None,
                updated_at: None,
            })
        });

        let service = DocumentService::new(
            Arc::new(mock_doc_repo),
            Arc::new(MockDocumentAssociationRepository::new()),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UpdateDocumentRequest {
            user_id: TEST_USER_ID.to_string(),
            document_id: "doc-123".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: Some("renamed doc".to_string()),
            metadata: None,
        };

        let result = service.update_document(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "renamed doc");
    }

    #[tokio::test]
    async fn test_permission_denied() {
        let mock_doc_repo = MockDocumentRepository::new();
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
