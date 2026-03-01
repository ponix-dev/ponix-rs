use crate::domain::document::Document;
use crate::domain::result::DomainResult;
use async_trait::async_trait;

/// Input for linking a document to a target entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkDocumentInput {
    pub document_id: String,
    pub target_id: String,
    pub organization_id: String,
    pub workspace_id: Option<String>,
}

/// Input for unlinking a document from a target entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkDocumentInput {
    pub document_id: String,
    pub target_id: String,
    pub organization_id: String,
}

/// Input for listing documents by target entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDocumentsByTargetInput {
    pub target_id: String,
    pub organization_id: String,
}

/// Repository trait for document association persistence operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait DocumentAssociationRepository: Send + Sync {
    /// Link a document to a data stream
    async fn link_to_data_stream(&self, input: LinkDocumentInput) -> DomainResult<()>;

    /// Unlink a document from a data stream
    async fn unlink_from_data_stream(&self, input: UnlinkDocumentInput) -> DomainResult<()>;

    /// List documents linked to a data stream
    async fn list_data_stream_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>>;

    /// Link a document to a data stream definition
    async fn link_to_definition(&self, input: LinkDocumentInput) -> DomainResult<()>;

    /// Unlink a document from a data stream definition
    async fn unlink_from_definition(&self, input: UnlinkDocumentInput) -> DomainResult<()>;

    /// List documents linked to a data stream definition
    async fn list_definition_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>>;

    /// Link a document to a workspace
    async fn link_to_workspace(&self, input: LinkDocumentInput) -> DomainResult<()>;

    /// Unlink a document from a workspace
    async fn unlink_from_workspace(&self, input: UnlinkDocumentInput) -> DomainResult<()>;

    /// List documents linked to a workspace
    async fn list_workspace_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>>;
}
