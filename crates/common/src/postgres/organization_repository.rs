use crate::domain::{
    CreateOrganizationRepoInputWithId, DeleteOrganizationRepoInput, DomainError, DomainResult,
    GetOrganizationRepoInput, GetUserOrganizationsRepoInput, ListOrganizationsRepoInput,
    Organization, OrganizationRepository, UpdateOrganizationRepoInput,
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
    #[instrument(skip(self, input), fields(organization_id = %input.id, user_id = %input.user_id))]
    async fn create_organization(
        &self,
        input: CreateOrganizationRepoInputWithId,
    ) -> DomainResult<Organization> {
        let mut conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Start transaction
        let transaction = conn
            .transaction()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        // Insert organization
        let org_result = transaction
            .execute(
                "INSERT INTO organizations (id, name, created_at, updated_at)
                 VALUES ($1, $2, $3, $4)",
                &[&input.id, &input.name, &now, &now],
            )
            .await;

        if let Err(e) = org_result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::OrganizationAlreadyExists(input.id.clone()));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        // Insert user-organization link
        let user_org_result = transaction
            .execute(
                "INSERT INTO user_organizations (user_id, organization_id, created_at)
                 VALUES ($1, $2, $3)",
                &[&input.user_id, &input.id, &now],
            )
            .await;

        if let Err(e) = user_org_result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::UserOrganizationAlreadyExists(
                        input.user_id.clone(),
                        input.id.clone(),
                    ));
                }
                // Foreign key violation (user doesn't exist)
                if db_err.code().code() == "23503" {
                    return Err(DomainError::UserNotFound(input.user_id.clone()));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        // Commit transaction
        transaction
            .commit()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        debug!(
            organization_id = %input.id,
            user_id = %input.user_id,
            "organization created with user link in database"
        );

        Ok(Organization {
            id: input.id,
            name: input.name,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn get_organization(
        &self,
        input: GetOrganizationRepoInput,
    ) -> DomainResult<Option<Organization>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(organization_id = %input.organization_id, "fetching organization from database");

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

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn update_organization(
        &self,
        input: UpdateOrganizationRepoInput,
    ) -> DomainResult<Organization> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(organization_id = %input.organization_id, "updating organization in database");

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

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn delete_organization(&self, input: DeleteOrganizationRepoInput) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        debug!(organization_id = %input.organization_id, "soft deleting organization in database");

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
        _input: ListOrganizationsRepoInput,
    ) -> DomainResult<Vec<Organization>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!("listing all active organizations from database");

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

    #[instrument(skip(self, input), fields(user_id = %input.user_id))]
    async fn get_organizations_by_user_id(
        &self,
        input: GetUserOrganizationsRepoInput,
    ) -> DomainResult<Vec<Organization>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(user_id = %input.user_id, "fetching organizations for user from database");

        let rows = conn
            .query(
                "SELECT o.id, o.name, o.deleted_at, o.created_at, o.updated_at
                 FROM organizations o
                 INNER JOIN user_organizations uo ON o.id = uo.organization_id
                 WHERE uo.user_id = $1 AND o.deleted_at IS NULL
                 ORDER BY o.created_at DESC",
                &[&input.user_id],
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
