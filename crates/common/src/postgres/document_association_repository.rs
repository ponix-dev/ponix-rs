use crate::domain::{
    Document, DocumentAssociationRepository, DomainError, DomainResult, LinkDocumentInput,
    ListDocumentsByTargetInput, UnlinkDocumentInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::{debug, instrument};

const DOCUMENT_SELECT_COLUMNS: &str = "d.document_id, d.organization_id, d.name, d.mime_type, d.size_bytes, d.object_store_key, d.checksum, d.metadata, d.deleted_at, d.created_at, d.updated_at";

fn document_from_row(row: &tokio_postgres::Row) -> Document {
    let deleted_at: Option<DateTime<Utc>> = row.get(8);
    let created_at: DateTime<Utc> = row.get(9);
    let updated_at: DateTime<Utc> = row.get(10);
    Document {
        document_id: row.get(0),
        organization_id: row.get(1),
        name: row.get(2),
        mime_type: row.get(3),
        size_bytes: row.get(4),
        object_store_key: row.get(5),
        checksum: row.get(6),
        metadata: row.get(7),
        deleted_at,
        created_at: Some(created_at),
        updated_at: Some(updated_at),
    }
}

#[derive(Clone)]
pub struct PostgresDocumentAssociationRepository {
    client: PostgresClient,
}

impl PostgresDocumentAssociationRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }

    /// Execute a transactional link: INSERT into junction table + MERGE AGE graph edge
    async fn link_transaction(
        &self,
        insert_sql: &str,
        insert_params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
        source_label: &str,
        source_id: &str,
        document_id: &str,
    ) -> DomainResult<()> {
        let mut conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let tx = conn
            .transaction()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        // Insert into junction table
        let result = tx.execute(insert_sql, insert_params).await;
        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::DocumentAssociationAlreadyExists(format!(
                        "Association already exists: document {} <-> {}",
                        document_id, source_id
                    )));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        // Create AGE graph edge
        let cypher = format!(
            "SELECT * FROM ag_catalog.cypher('ponix_graph', $$ \
             MERGE (s:{} {{id: '{}'}}) \
             MERGE (d:Document {{id: '{}'}}) \
             CREATE (s)-[:HAS_DOCUMENT]->(d) \
             $$) AS (result ag_catalog.agtype)",
            source_label, source_id, document_id
        );

        tx.batch_execute(&format!("LOAD 'age'; SET search_path = ag_catalog, \"$user\", public; {}; SET search_path = \"$user\", public;", cypher))
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        Ok(())
    }

    /// Execute a transactional unlink: DELETE from junction table + DELETE AGE graph edge
    async fn unlink_transaction(
        &self,
        delete_sql: &str,
        delete_params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
        source_label: &str,
        source_id: &str,
        document_id: &str,
    ) -> DomainResult<()> {
        let mut conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let tx = conn
            .transaction()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        // Delete from junction table
        let rows_affected = tx
            .execute(delete_sql, delete_params)
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::DocumentNotFound(format!(
                "Association not found: document {} <-> {}",
                document_id, source_id
            )));
        }

        // Delete AGE graph edge
        let cypher = format!(
            "SELECT * FROM ag_catalog.cypher('ponix_graph', $$ \
             MATCH (s:{} {{id: '{}'}})-[e:HAS_DOCUMENT]->(d:Document {{id: '{}'}}) \
             DELETE e \
             $$) AS (result ag_catalog.agtype)",
            source_label, source_id, document_id
        );

        tx.batch_execute(&format!("LOAD 'age'; SET search_path = ag_catalog, \"$user\", public; {}; SET search_path = \"$user\", public;", cypher))
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        Ok(())
    }

    /// List documents from a junction table join
    async fn list_documents_by_join(
        &self,
        query: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> DomainResult<Vec<Document>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(query, params)
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        Ok(rows.iter().map(document_from_row).collect())
    }
}

#[async_trait]
impl DocumentAssociationRepository for PostgresDocumentAssociationRepository {
    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn link_to_data_stream(&self, input: LinkDocumentInput) -> DomainResult<()> {
        let workspace_id = input
            .workspace_id
            .as_deref()
            .unwrap_or_default()
            .to_string();
        debug!(document_id = %input.document_id, data_stream_id = %input.target_id, "linking document to data stream");

        self.link_transaction(
            "INSERT INTO document_data_streams (document_id, data_stream_id, organization_id, workspace_id, created_at) VALUES ($1, $2, $3, $4, NOW())",
            &[&input.document_id, &input.target_id, &input.organization_id, &workspace_id],
            "DataStream",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn unlink_from_data_stream(&self, input: UnlinkDocumentInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, data_stream_id = %input.target_id, "unlinking document from data stream");

        self.unlink_transaction(
            "DELETE FROM document_data_streams WHERE document_id = $1 AND data_stream_id = $2 AND organization_id = $3",
            &[&input.document_id, &input.target_id, &input.organization_id],
            "DataStream",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn list_data_stream_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>> {
        debug!(data_stream_id = %input.target_id, "listing documents for data stream");

        self.list_documents_by_join(
            &format!(
                "SELECT {} FROM documents d \
                 INNER JOIN document_data_streams dds ON d.document_id = dds.document_id \
                 WHERE dds.data_stream_id = $1 AND dds.organization_id = $2 AND d.deleted_at IS NULL \
                 ORDER BY d.created_at DESC",
                DOCUMENT_SELECT_COLUMNS
            ),
            &[&input.target_id, &input.organization_id],
        )
        .await
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn link_to_definition(&self, input: LinkDocumentInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, definition_id = %input.target_id, "linking document to definition");

        self.link_transaction(
            "INSERT INTO document_definitions (document_id, definition_id, organization_id, created_at) VALUES ($1, $2, $3, NOW())",
            &[&input.document_id, &input.target_id, &input.organization_id],
            "DataStreamDefinition",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn unlink_from_definition(&self, input: UnlinkDocumentInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, definition_id = %input.target_id, "unlinking document from definition");

        self.unlink_transaction(
            "DELETE FROM document_definitions WHERE document_id = $1 AND definition_id = $2 AND organization_id = $3",
            &[&input.document_id, &input.target_id, &input.organization_id],
            "DataStreamDefinition",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn list_definition_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>> {
        debug!(definition_id = %input.target_id, "listing documents for definition");

        self.list_documents_by_join(
            &format!(
                "SELECT {} FROM documents d \
                 INNER JOIN document_definitions dd ON d.document_id = dd.document_id \
                 WHERE dd.definition_id = $1 AND dd.organization_id = $2 AND d.deleted_at IS NULL \
                 ORDER BY d.created_at DESC",
                DOCUMENT_SELECT_COLUMNS
            ),
            &[&input.target_id, &input.organization_id],
        )
        .await
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn link_to_workspace(&self, input: LinkDocumentInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, workspace_id = %input.target_id, "linking document to workspace");

        self.link_transaction(
            "INSERT INTO document_workspaces (document_id, workspace_id, organization_id, created_at) VALUES ($1, $2, $3, NOW())",
            &[&input.document_id, &input.target_id, &input.organization_id],
            "Workspace",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(document_id = %input.document_id, target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn unlink_from_workspace(&self, input: UnlinkDocumentInput) -> DomainResult<()> {
        debug!(document_id = %input.document_id, workspace_id = %input.target_id, "unlinking document from workspace");

        self.unlink_transaction(
            "DELETE FROM document_workspaces WHERE document_id = $1 AND workspace_id = $2 AND organization_id = $3",
            &[&input.document_id, &input.target_id, &input.organization_id],
            "Workspace",
            &input.target_id,
            &input.document_id,
        )
        .await
    }

    #[instrument(skip(self, input), fields(target_id = %input.target_id, organization_id = %input.organization_id))]
    async fn list_workspace_documents(
        &self,
        input: ListDocumentsByTargetInput,
    ) -> DomainResult<Vec<Document>> {
        debug!(workspace_id = %input.target_id, "listing documents for workspace");

        self.list_documents_by_join(
            &format!(
                "SELECT {} FROM documents d \
                 INNER JOIN document_workspaces dw ON d.document_id = dw.document_id \
                 WHERE dw.workspace_id = $1 AND dw.organization_id = $2 AND d.deleted_at IS NULL \
                 ORDER BY d.created_at DESC",
                DOCUMENT_SELECT_COLUMNS
            ),
            &[&input.target_id, &input.organization_id],
        )
        .await
    }
}
