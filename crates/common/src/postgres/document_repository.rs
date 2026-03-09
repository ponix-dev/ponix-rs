use crate::domain::{
    CreateDocumentRepoInputWithId, DeleteDocumentRepoInput, Document, DocumentRepository,
    DomainError, DomainResult, GetDocumentRepoInput, ListDocumentsRepoInput,
    UpdateDocumentRepoInput,
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
    pub yrs_state: Vec<u8>,
    pub yrs_state_vector: Vec<u8>,
    pub content_text: String,
    pub content_html: String,
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
            yrs_state: row.yrs_state,
            yrs_state_vector: row.yrs_state_vector,
            content_text: row.content_text,
            content_html: row.content_html,
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
        yrs_state: row.get(3),
        yrs_state_vector: row.get(4),
        content_text: row.get(5),
        content_html: row.get(6),
        metadata: row.get(7),
        deleted_at: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
    }
}

const SELECT_COLUMNS: &str = "document_id, organization_id, name, yrs_state, yrs_state_vector, content_text, content_html, metadata, deleted_at, created_at, updated_at";

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
    async fn create_document(
        &self,
        input: CreateDocumentRepoInputWithId,
    ) -> DomainResult<Document> {
        debug!(document_id = %input.document_id, "creating document in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO documents (document_id, organization_id, name, yrs_state, yrs_state_vector, content_text, content_html, metadata, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &input.document_id,
                    &input.organization_id,
                    &input.name,
                    &input.yrs_state,
                    &input.yrs_state_vector,
                    &input.content_text,
                    &input.content_html,
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
            yrs_state: input.yrs_state,
            yrs_state_vector: input.yrs_state_vector,
            content_text: input.content_text,
            content_html: input.content_html,
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

    #[instrument(skip(self), fields(document_id = %document_id))]
    async fn get_document_by_id(&self, document_id: &str) -> DomainResult<Option<Document>> {
        debug!(document_id = %document_id, "getting document by id from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                &format!(
                    "SELECT {} FROM documents WHERE document_id = $1 AND deleted_at IS NULL",
                    SELECT_COLUMNS
                ),
                &[&document_id],
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
        let (yrs_state, yrs_state_vector) = crate::yrs::create_empty_document();
        let row = DocumentRow {
            document_id: "doc-001".to_string(),
            organization_id: "org-001".to_string(),
            name: "Manual".to_string(),
            yrs_state: yrs_state.clone(),
            yrs_state_vector: yrs_state_vector.clone(),
            content_text: String::new(),
            content_html: String::new(),
            metadata: serde_json::json!({"author": "test"}),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };

        let document: Document = row.into();

        assert_eq!(document.document_id, "doc-001");
        assert_eq!(document.organization_id, "org-001");
        assert_eq!(document.name, "Manual");
        assert_eq!(document.yrs_state, yrs_state);
        assert_eq!(document.yrs_state_vector, yrs_state_vector);
        assert!(document.content_text.is_empty());
        assert!(document.content_html.is_empty());
        assert_eq!(document.metadata, serde_json::json!({"author": "test"}));
        assert!(document.deleted_at.is_none());
        assert_eq!(document.created_at, Some(now));
        assert_eq!(document.updated_at, Some(now));
    }

    #[test]
    fn test_document_row_with_deleted_at() {
        let now = Utc::now();
        let (yrs_state, yrs_state_vector) = crate::yrs::create_empty_document();
        let row = DocumentRow {
            document_id: "doc-002".to_string(),
            organization_id: "org-001".to_string(),
            name: "Deleted".to_string(),
            yrs_state,
            yrs_state_vector,
            content_text: String::new(),
            content_html: String::new(),
            metadata: serde_json::json!({}),
            deleted_at: Some(now),
            created_at: now,
            updated_at: now,
        };

        let document: Document = row.into();
        assert_eq!(document.deleted_at, Some(now));
    }
}
