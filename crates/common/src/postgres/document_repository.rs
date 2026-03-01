use crate::domain::{
    CreateDocumentRepoInput, DeleteDocumentRepoInput, Document, DocumentRepository, DomainError,
    DomainResult, GetDocumentRepoInput, ListDocumentsRepoInput, UpdateDocumentRepoInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Document row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRow {
    pub document_id: String,
    pub organization_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub object_store_key: String,
    pub checksum: String,
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<DocumentRow> for Document {
    fn from(row: DocumentRow) -> Self {
        Document {
            document_id: row.document_id,
            organization_id: row.organization_id,
            name: row.name,
            mime_type: row.mime_type,
            size_bytes: row.size_bytes,
            object_store_key: row.object_store_key,
            checksum: row.checksum,
            metadata: row.metadata,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

fn row_from_pg(row: &tokio_postgres::Row) -> DocumentRow {
    DocumentRow {
        document_id: row.get(0),
        organization_id: row.get(1),
        name: row.get(2),
        mime_type: row.get(3),
        size_bytes: row.get(4),
        object_store_key: row.get(5),
        checksum: row.get(6),
        metadata: row.get(7),
        deleted_at: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
    }
}

const SELECT_COLUMNS: &str = "document_id, organization_id, name, mime_type, size_bytes, object_store_key, checksum, metadata, deleted_at, created_at, updated_at";

#[derive(Clone)]
pub struct PostgresDocumentRepository {
    client: PostgresClient,
}

impl PostgresDocumentRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl DocumentRepository for PostgresDocumentRepository {
    #[instrument(skip(self, input), fields(document_id = %input.document_id, organization_id = %input.organization_id))]
    async fn create_document(&self, input: CreateDocumentRepoInput) -> DomainResult<Document> {
        debug!(document_id = %input.document_id, "creating document in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO documents (document_id, organization_id, name, mime_type, size_bytes, object_store_key, checksum, metadata, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &input.document_id,
                    &input.organization_id,
                    &input.name,
                    &input.mime_type,
                    &input.size_bytes,
                    &input.object_store_key,
                    &input.checksum,
                    &input.metadata,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                match db_err.code().code() {
                    "23505" => {
                        return Err(DomainError::DocumentAlreadyExists(input.document_id));
                    }
                    "23503" => {
                        return Err(DomainError::OrganizationNotFound(input.organization_id));
                    }
                    _ => {}
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!(document_id = %input.document_id, "document created in database");

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
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, organization_id = %input.organization_id))]
    async fn get_document(&self, input: GetDocumentRepoInput) -> DomainResult<Option<Document>> {
        debug!(document_id = %input.document_id, organization_id = %input.organization_id, "getting document from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                &format!(
                    "SELECT {} FROM documents WHERE document_id = $1 AND organization_id = $2 AND deleted_at IS NULL",
                    SELECT_COLUMNS
                ),
                &[&input.document_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let document = row.map(|r| {
            let doc_row = row_from_pg(&r);
            doc_row.into()
        });

        Ok(document)
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, organization_id = %input.organization_id))]
    async fn update_document(&self, input: UpdateDocumentRepoInput) -> DomainResult<Document> {
        debug!(document_id = %input.document_id, organization_id = %input.organization_id, "updating document in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Build dynamic UPDATE query based on provided fields
        let mut query = String::from("UPDATE documents SET updated_at = $1");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        let mut param_idx = 2;

        if let Some(ref name) = input.name {
            query.push_str(&format!(", name = ${}", param_idx));
            params.push(name);
            param_idx += 1;
        }

        if let Some(ref mime_type) = input.mime_type {
            query.push_str(&format!(", mime_type = ${}", param_idx));
            params.push(mime_type);
            param_idx += 1;
        }

        if let Some(ref size_bytes) = input.size_bytes {
            query.push_str(&format!(", size_bytes = ${}", param_idx));
            params.push(size_bytes);
            param_idx += 1;
        }

        if let Some(ref checksum) = input.checksum {
            query.push_str(&format!(", checksum = ${}", param_idx));
            params.push(checksum);
            param_idx += 1;
        }

        if let Some(ref metadata) = input.metadata {
            query.push_str(&format!(", metadata = ${}", param_idx));
            params.push(metadata);
            param_idx += 1;
        }

        query.push_str(&format!(
            " WHERE document_id = ${} AND organization_id = ${} AND deleted_at IS NULL RETURNING {}",
            param_idx,
            param_idx + 1,
            SELECT_COLUMNS
        ));
        params.push(&input.document_id);
        params.push(&input.organization_id);

        let row = conn
            .query_opt(&query, &params[..])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(r) => {
                let doc_row = row_from_pg(&r);
                debug!(document_id = %doc_row.document_id, "document updated in database");
                Ok(doc_row.into())
            }
            None => Err(DomainError::DocumentNotFound(input.document_id)),
        }
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, organization_id = %input.organization_id))]
    async fn delete_document(&self, input: DeleteDocumentRepoInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, organization_id = %input.organization_id, "soft deleting document");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let rows_affected = conn
            .execute(
                "UPDATE documents
                 SET deleted_at = $1, updated_at = $1
                 WHERE document_id = $2 AND organization_id = $3 AND deleted_at IS NULL",
                &[&now, &input.document_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::DocumentNotFound(input.document_id));
        }

        debug!(document_id = %input.document_id, "document soft deleted");
        Ok(())
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn list_documents(&self, input: ListDocumentsRepoInput) -> DomainResult<Vec<Document>> {
        debug!(organization_id = %input.organization_id, "listing documents from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                &format!(
                    "SELECT {} FROM documents WHERE organization_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
                    SELECT_COLUMNS
                ),
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let documents: Vec<Document> = rows
            .into_iter()
            .map(|r| {
                let doc_row = row_from_pg(&r);
                doc_row.into()
            })
            .collect();

        debug!(count = documents.len(), "listed documents from database");
        Ok(documents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_row_to_document_conversion() {
        let now = Utc::now();
        let row = DocumentRow {
            document_id: "doc-001".to_string(),
            organization_id: "org-001".to_string(),
            name: "Manual.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
            object_store_key: "org-001/doc-001/Manual.pdf".to_string(),
            checksum: "abc123".to_string(),
            metadata: serde_json::json!({"author": "test"}),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };

        let document: Document = row.into();

        assert_eq!(document.document_id, "doc-001");
        assert_eq!(document.organization_id, "org-001");
        assert_eq!(document.name, "Manual.pdf");
        assert_eq!(document.mime_type, "application/pdf");
        assert_eq!(document.size_bytes, 1024);
        assert_eq!(document.object_store_key, "org-001/doc-001/Manual.pdf");
        assert_eq!(document.checksum, "abc123");
        assert_eq!(document.metadata, serde_json::json!({"author": "test"}));
        assert!(document.deleted_at.is_none());
        assert_eq!(document.created_at, Some(now));
        assert_eq!(document.updated_at, Some(now));
    }

    #[test]
    fn test_document_row_with_deleted_at() {
        let now = Utc::now();
        let row = DocumentRow {
            document_id: "doc-002".to_string(),
            organization_id: "org-001".to_string(),
            name: "Deleted.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 512,
            object_store_key: "org-001/doc-002/Deleted.pdf".to_string(),
            checksum: "def456".to_string(),
            metadata: serde_json::json!({}),
            deleted_at: Some(now),
            created_at: now,
            updated_at: now,
        };

        let document: Document = row.into();
        assert_eq!(document.deleted_at, Some(now));
    }
}
