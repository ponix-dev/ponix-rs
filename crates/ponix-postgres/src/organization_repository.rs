use async_trait::async_trait;
use chrono::Utc;
use tracing::debug;

use ponix_domain::{
    CreateOrganizationInputWithId, DeleteOrganizationInput, DomainError, DomainResult,
    GetOrganizationInput, ListOrganizationsInput, Organization, OrganizationRepository,
    UpdateOrganizationInput,
};

use crate::client::PostgresClient;

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
                let org = Organization {
                    id: row.get("id"),
                    name: row.get("name"),
                    deleted_at: row.get("deleted_at"),
                    created_at: Some(row.get("created_at")),
                    updated_at: Some(row.get("updated_at")),
                };
                Ok(Some(org))
            }
            None => Ok(None),
        }
    }

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
                let org = Organization {
                    id: row.get("id"),
                    name: row.get("name"),
                    deleted_at: row.get("deleted_at"),
                    created_at: Some(row.get("created_at")),
                    updated_at: Some(row.get("updated_at")),
                };
                Ok(org)
            }
            None => Err(DomainError::OrganizationNotFound(
                input.organization_id.clone(),
            )),
        }
    }

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
            .map(|row| Organization {
                id: row.get("id"),
                name: row.get("name"),
                deleted_at: row.get("deleted_at"),
                created_at: Some(row.get("created_at")),
                updated_at: Some(row.get("updated_at")),
            })
            .collect();

        Ok(organizations)
    }
}
