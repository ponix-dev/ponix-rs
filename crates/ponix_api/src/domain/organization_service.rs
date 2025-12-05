use common::domain::{
    CreateOrganizationInput, CreateOrganizationInputWithId, DeleteOrganizationInput, DomainError,
    DomainResult, GetOrganizationInput, ListOrganizationsInput, Organization,
    OrganizationRepository, UpdateOrganizationInput,
};
use std::sync::Arc;
use tracing::{debug, instrument};

/// Domain service for organization business logic
pub struct OrganizationService {
    repository: Arc<dyn OrganizationRepository>,
}

impl OrganizationService {
    pub fn new(repository: Arc<dyn OrganizationRepository>) -> Self {
        Self { repository }
    }

    /// Create a new organization with generated ID
    #[instrument(skip(self, input), fields(name = %input.name, user_id = %input.user_id))]
    pub async fn create_organization(
        &self,
        input: CreateOrganizationInput,
    ) -> DomainResult<Organization> {
        debug!(name = %input.name, user_id = %input.user_id, "creating organization");

        // Validate name is not empty
        if input.name.trim().is_empty() {
            return Err(DomainError::InvalidOrganizationName(
                "Organization name cannot be empty".to_string(),
            ));
        }

        // Validate user_id is not empty (mandatory field)
        if input.user_id.trim().is_empty() {
            return Err(DomainError::InvalidUserId(
                "User ID cannot be empty".to_string(),
            ));
        }

        // Generate unique organization ID using xid
        let organization_id = xid::new().to_string();

        let repo_input = CreateOrganizationInputWithId {
            id: organization_id.clone(),
            name: input.name,
            user_id: input.user_id,
        };

        let organization = self.repository.create_organization(repo_input).await?;

        debug!(organization_id = %organization.id, "organization created successfully");
        Ok(organization)
    }

    /// Get organization by ID (excludes soft deleted)
    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    pub async fn get_organization(
        &self,
        input: GetOrganizationInput,
    ) -> DomainResult<Organization> {
        debug!(organization_id = %input.organization_id, "getting organization");

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        let organization = self
            .repository
            .get_organization(input.clone())
            .await?
            .ok_or_else(|| DomainError::OrganizationNotFound(input.organization_id.clone()))?;

        Ok(organization)
    }

    /// Update organization name
    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    pub async fn update_organization(
        &self,
        input: UpdateOrganizationInput,
    ) -> DomainResult<Organization> {
        debug!(organization_id = %input.organization_id, "updating organization");

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        if input.name.trim().is_empty() {
            return Err(DomainError::InvalidOrganizationName(
                "Organization name cannot be empty".to_string(),
            ));
        }

        let organization = self.repository.update_organization(input).await?;

        debug!(organization_id = %organization.id, "organization updated successfully");
        Ok(organization)
    }

    /// Soft delete organization
    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    pub async fn delete_organization(&self, input: DeleteOrganizationInput) -> DomainResult<()> {
        debug!(organization_id = %input.organization_id, "Deleting organization");

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        self.repository.delete_organization(input.clone()).await?;

        debug!(organization_id = %input.organization_id, "Organization soft deleted successfully");
        Ok(())
    }

    /// List all active organizations (excludes soft deleted)
    #[instrument(skip(self, input))]
    pub async fn list_organizations(
        &self,
        input: ListOrganizationsInput,
    ) -> DomainResult<Vec<Organization>> {
        debug!("Listing organizations");

        let organizations = self.repository.list_organizations(input).await?;

        debug!(count = organizations.len(), "Listed organizations");
        Ok(organizations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::MockOrganizationRepository;

    #[tokio::test]
    async fn test_create_organization_success() {
        let mut mock_repo = MockOrganizationRepository::new();

        let expected_org = Organization {
            id: "generated-id".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_repo
            .expect_create_organization()
            .withf(|input: &CreateOrganizationInputWithId| {
                !input.id.is_empty()
                    && input.name == "Test Org"
                    && input.user_id == "user-123"
            })
            .times(1)
            .return_once(move |_| Ok(expected_org.clone()));

        let service = OrganizationService::new(Arc::new(mock_repo));
        let input = CreateOrganizationInput {
            name: "Test Org".to_string(),
            user_id: "user-123".to_string(),
        };

        let result = service.create_organization(input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "Test Org");
    }

    #[tokio::test]
    async fn test_create_organization_empty_name() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo));

        let input = CreateOrganizationInput {
            name: "".to_string(),
            user_id: "user-123".to_string(),
        };

        let result = service.create_organization(input).await;
        assert!(matches!(
            result,
            Err(DomainError::InvalidOrganizationName(_))
        ));
    }

    #[tokio::test]
    async fn test_create_organization_empty_user_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo));

        let input = CreateOrganizationInput {
            name: "Test Org".to_string(),
            user_id: "".to_string(),
        };

        let result = service.create_organization(input).await;
        assert!(matches!(result, Err(DomainError::InvalidUserId(_))));
    }

    #[tokio::test]
    async fn test_get_organization_not_found() {
        let mut mock_repo = MockOrganizationRepository::new();

        mock_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = OrganizationService::new(Arc::new(mock_repo));
        let input = GetOrganizationInput {
            organization_id: "nonexistent".to_string(),
        };

        let result = service.get_organization(input).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_organization_empty_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo));

        let input = GetOrganizationInput {
            organization_id: "".to_string(),
        };

        let result = service.get_organization(input).await;
        assert!(matches!(result, Err(DomainError::InvalidOrganizationId(_))));
    }

    #[tokio::test]
    async fn test_update_organization_empty_name() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo));

        let input = UpdateOrganizationInput {
            organization_id: "org-123".to_string(),
            name: "".to_string(),
        };

        let result = service.update_organization(input).await;
        assert!(matches!(
            result,
            Err(DomainError::InvalidOrganizationName(_))
        ));
    }

    #[tokio::test]
    async fn test_delete_organization_empty_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo));

        let input = DeleteOrganizationInput {
            organization_id: "".to_string(),
        };

        let result = service.delete_organization(input).await;
        assert!(matches!(result, Err(DomainError::InvalidOrganizationId(_))));
    }
}
