use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Document entity representing metadata about a stored document
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub document_id: String,
    pub organization_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub object_store_key: String,
    pub checksum: String,
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a document
#[derive(Debug, Clone, PartialEq)]
pub struct CreateDocumentRepoInput {
    pub document_id: String,
    pub organization_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub object_store_key: String,
    pub checksum: String,
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

/// Repository trait for document persistence operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait DocumentRepository: Send + Sync {
    /// Create a new document
    async fn create_document(&self, input: CreateDocumentRepoInput) -> DomainResult<Document>;

    /// Get a document by ID and organization (excludes soft deleted)
    async fn get_document(&self, input: GetDocumentRepoInput) -> DomainResult<Option<Document>>;

    /// Update a document's name and/or metadata
    async fn update_document(&self, input: UpdateDocumentRepoInput) -> DomainResult<Document>;

    /// Soft delete a document
    async fn delete_document(&self, input: DeleteDocumentRepoInput) -> DomainResult<()>;

    /// List documents by organization (excludes soft deleted)
    async fn list_documents(&self, input: ListDocumentsRepoInput) -> DomainResult<Vec<Document>>;
}
