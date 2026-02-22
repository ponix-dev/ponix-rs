use crate::domain::{
    CreateWorkspaceRepoInputWithId, DeleteWorkspaceRepoInput, DomainError, DomainResult,
    GetWorkspaceRepoInput, ListWorkspacesRepoInput, UpdateWorkspaceRepoInput, Workspace,
    WorkspaceRepository,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Workspace row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub organization_id: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Workspace {
            id: row.id,
            name: row.name,
            organization_id: row.organization_id,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// PostgreSQL implementation of WorkspaceRepository trait
#[derive(Clone)]
pub struct PostgresWorkspaceRepository {
    client: PostgresClient,
}

impl PostgresWorkspaceRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WorkspaceRepository for PostgresWorkspaceRepository {
    #[instrument(skip(self, input), fields(workspace_id = %input.id, organization_id = %input.organization_id))]
    async fn create_workspace(
        &self,
        input: CreateWorkspaceRepoInputWithId,
    ) -> DomainResult<Workspace> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO workspaces (id, name, organization_id, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5)",
                &[&input.id, &input.name, &input.organization_id, &now, &now],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                // Unique constraint violation
                if db_err.code().code() == "23505" {
                    return Err(DomainError::WorkspaceAlreadyExists(input.id.clone()));
                }
                // Foreign key violation (organization doesn't exist)
                if db_err.code().code() == "23503" {
                    return Err(DomainError::OrganizationNotFound(
                        input.organization_id.clone(),
                    ));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!(
            workspace_id = %input.id,
            organization_id = %input.organization_id,
            "workspace created in database"
        );

        Ok(Workspace {
            id: input.id,
            name: input.name,
            organization_id: input.organization_id,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(workspace_id = %input.workspace_id, organization_id = %input.organization_id))]
    async fn get_workspace(&self, input: GetWorkspaceRepoInput) -> DomainResult<Option<Workspace>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(workspace_id = %input.workspace_id, "fetching workspace from database");

        let row = conn
            .query_opt(
                "SELECT id, name, organization_id, deleted_at, created_at, updated_at
                 FROM workspaces
                 WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL",
                &[&input.workspace_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let ws_row = WorkspaceRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    organization_id: row.get("organization_id"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(Some(ws_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(workspace_id = %input.workspace_id, organization_id = %input.organization_id))]
    async fn update_workspace(&self, input: UpdateWorkspaceRepoInput) -> DomainResult<Workspace> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(workspace_id = %input.workspace_id, "updating workspace in database");

        let row = conn
            .query_opt(
                "UPDATE workspaces
                 SET name = $1, updated_at = $2
                 WHERE id = $3 AND organization_id = $4 AND deleted_at IS NULL
                 RETURNING id, name, organization_id, deleted_at, created_at, updated_at",
                &[
                    &input.name,
                    &now,
                    &input.workspace_id,
                    &input.organization_id,
                ],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let ws_row = WorkspaceRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    organization_id: row.get("organization_id"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(ws_row.into())
            }
            None => Err(DomainError::WorkspaceNotFound(input.workspace_id.clone())),
        }
    }

    #[instrument(skip(self, input), fields(workspace_id = %input.workspace_id, organization_id = %input.organization_id))]
    async fn delete_workspace(&self, input: DeleteWorkspaceRepoInput) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(workspace_id = %input.workspace_id, "soft deleting workspace in database");

        let rows_affected = conn
            .execute(
                "UPDATE workspaces
                 SET deleted_at = $1, updated_at = $1
                 WHERE id = $2 AND organization_id = $3 AND deleted_at IS NULL",
                &[&now, &input.workspace_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::WorkspaceNotFound(input.workspace_id.clone()));
        }

        Ok(())
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn list_workspaces(
        &self,
        input: ListWorkspacesRepoInput,
    ) -> DomainResult<Vec<Workspace>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(organization_id = %input.organization_id, "listing workspaces from database");

        let rows = conn
            .query(
                "SELECT id, name, organization_id, deleted_at, created_at, updated_at
                 FROM workspaces
                 WHERE organization_id = $1 AND deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let workspaces = rows
            .into_iter()
            .map(|row| {
                let ws_row = WorkspaceRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    organization_id: row.get("organization_id"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                ws_row.into()
            })
            .collect();

        Ok(workspaces)
    }
}
