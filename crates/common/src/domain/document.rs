use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Document entity representing a collaborative document with Yrs CRDT state
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub document_id: String,
    pub organization_id: String,
    pub name: String,
    pub yrs_state: Vec<u8>,
    pub yrs_state_vector: Vec<u8>,
    pub content_text: String,
    pub content_html: String,
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a document
#[derive(Debug, Clone, PartialEq)]
pub struct CreateDocumentRepoInputWithId {
    pub document_id: String,
    pub organization_id: String,
    pub name: String,
    pub yrs_state: Vec<u8>,
    pub yrs_state_vector: Vec<u8>,
    pub content_text: String,
    pub content_html: String,
    pub metadata: serde_json::Value,
}

/// Repository input for getting a document by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetDocumentRepoInput {
    pub document_id: String,
    pub organization_id: String,
}

/// Repository input for updating a document
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateDocumentRepoInput {
    pub document_id: String,
    pub organization_id: String,
    pub name: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Repository input for deleting a document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteDocumentRepoInput {
    pub document_id: String,
    pub organization_id: String,
}

/// Repository input for listing documents by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDocumentsRepoInput {
    pub organization_id: String,
}

/// Repository input for updating Yrs CRDT state (used by Document Snapshotter)
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateYrsStateInput {
    pub document_id: String,
    pub organization_id: String,
    pub yrs_state: Vec<u8>,
    pub yrs_state_vector: Vec<u8>,
    pub content_text: String,
    pub content_html: String,
}

/// Repository trait for document persistence operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait DocumentRepository: Send + Sync {
    /// Create a new document
    async fn create_document(&self, input: CreateDocumentRepoInputWithId)
        -> DomainResult<Document>;

    /// Get a document by ID and organization (excludes soft deleted)
    async fn get_document(&self, input: GetDocumentRepoInput) -> DomainResult<Option<Document>>;

    /// Get a document by ID only, without organization scoping (excludes soft deleted).
    /// Used by the collaboration server where organization context is not available.
    async fn get_document_by_id(&self, document_id: &str) -> DomainResult<Option<Document>>;

    /// Update a document's name and/or metadata
    async fn update_document(&self, input: UpdateDocumentRepoInput) -> DomainResult<Document>;

    /// Soft delete a document
    async fn delete_document(&self, input: DeleteDocumentRepoInput) -> DomainResult<()>;

    /// List documents by organization (excludes soft deleted)
    async fn list_documents(&self, input: ListDocumentsRepoInput) -> DomainResult<Vec<Document>>;

    /// Update Yrs CRDT state with advisory lock protection.
    /// Returns `true` if write succeeded, `false` if advisory lock was not acquired.
    async fn update_yrs_state(&self, input: UpdateYrsStateInput) -> DomainResult<bool>;
}
