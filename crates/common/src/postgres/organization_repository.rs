use crate::domain::{
    CreateOrganizationInputWithId, DeleteOrganizationInput, DomainError, DomainResult,
    GetOrganizationInput, ListOrganizationsInput, Organization, OrganizationRepository,
    UpdateOrganizationInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Organization row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationRow {
    pub id: String,
    pub name: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<OrganizationRow> for Organization {
    fn from(row: OrganizationRow) -> Self {
        Organization {
            id: row.id,
            name: row.name,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// PostgreSQL implementation of OrganizationRepository trait
#[derive(Clone)]
pub struct PostgresOrganizationRepository {
    client: PostgresClient,
}

impl PostgresOrganizationRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl OrganizationRepository for PostgresOrganizationRepository {
    #[instrument(skip(self), fields(organization_id = %input.id))]
    async fn create_organization(
        &self,
        input: CreateOrganizationInputWithId,
    ) -> DomainResult<Organization> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Execute insert
        let result = conn
            .execute(
                "INSERT INTO organizations (id, name, created_at, updated_at)
                 VALUES ($1, $2, $3, $4)",
                &[&input.id, &input.name, &now, &now],
            )
            .await;

        // Handle unique constraint violation
        if let Err(e) = result {
            // Check if it's a database error with a unique constraint violation
            if let Some(db_err) = e.as_db_error() {
                // PostgreSQL error code 23505 is unique_violation
                if db_err.code().code() == "23505" {
                    return Err(DomainError::OrganizationAlreadyExists(input.id.clone()));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!(organization_id = %input.id, "Organization created in database");

        // Return created organization
        Ok(Organization {
            id: input.id,
            name: input.name,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self), fields(organization_id = %input.organization_id))]
    async fn get_organization(
        &self,
        input: GetOrganizationInput,
    ) -> DomainResult<Option<Organization>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(organization_id = %input.organization_id, "Fetching organization from database");

        let row = conn
            .query_opt(
                "SELECT id, name, deleted_at, created_at, updated_at
                 FROM organizations
                 WHERE id = $1 AND deleted_at IS NULL",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let org_row = OrganizationRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(Some(org_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self), fields(organization_id = %input.organization_id))]
    async fn update_organization(
        &self,
        input: UpdateOrganizationInput,
    ) -> DomainResult<Organization> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(organization_id = %input.organization_id, "Updating organization in database");

        let row = conn
            .query_opt(
                "UPDATE organizations
                 SET name = $1, updated_at = $2
                 WHERE id = $3 AND deleted_at IS NULL
                 RETURNING id, name, deleted_at, created_at, updated_at",
                &[&input.name, &now, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let org_row = OrganizationRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(org_row.into())
            }
            None => Err(DomainError::OrganizationNotFound(
                input.organization_id.clone(),
            )),
        }
    }

    #[instrument(skip(self), fields(organization_id = %input.organization_id))]
    async fn delete_organization(&self, input: DeleteOrganizationInput) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(organization_id = %input.organization_id, "Soft deleting organization in database");

        let rows_affected = conn
            .execute(
                "UPDATE organizations
                 SET deleted_at = $1, updated_at = $1
                 WHERE id = $2 AND deleted_at IS NULL",
                &[&now, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::OrganizationNotFound(
                input.organization_id.clone(),
            ));
        }

        Ok(())
    }

    #[instrument(skip(self, _input))]
    async fn list_organizations(
        &self,
        _input: ListOrganizationsInput,
    ) -> DomainResult<Vec<Organization>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!("Listing all active organizations from database");

        let rows = conn
            .query(
                "SELECT id, name, deleted_at, created_at, updated_at
                 FROM organizations
                 WHERE deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let organizations = rows
            .into_iter()
            .map(|row| {
                let org_row = OrganizationRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    deleted_at: row.get("deleted_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                org_row.into()
            })
            .collect();

        Ok(organizations)
    }
}
