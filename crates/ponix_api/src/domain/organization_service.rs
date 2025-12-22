use common::auth::{Action, AuthorizationProvider, OrgRole, Resource};
use common::domain::{
    CreateOrganizationRepoInputWithId, DeleteOrganizationRepoInput, DomainError, DomainResult,
    GetOrganizationRepoInput, GetUserOrganizationsRepoInput, ListOrganizationsRepoInput,
    Organization, OrganizationRepository, UpdateOrganizationRepoInput,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

// ============================================================================
// Service Request Types
// ============================================================================
// These types are used by the service layer and embed user_id for authorization

/// Request to create an organization
#[derive(Debug, Clone, Validate)]
pub struct CreateOrganizationRequest {
    #[garde(length(min = 1))]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Request to get an organization by ID
#[derive(Debug, Clone, Validate)]
pub struct GetOrganizationRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Request to update an organization
#[derive(Debug, Clone, Validate)]
pub struct UpdateOrganizationRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Request to delete an organization
#[derive(Debug, Clone, Validate)]
pub struct DeleteOrganizationRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Request to list all organizations
#[derive(Debug, Clone, Validate)]
pub struct ListOrganizationsRequest {}

/// Request to get organizations for a user
#[derive(Debug, Clone, Validate)]
pub struct GetUserOrganizationsRequest {
    #[garde(length(min = 1))]
    pub user_id: String,
}

/// Domain service for organization business logic
pub struct OrganizationService {
    repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl OrganizationService {
    pub fn new(
        repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            repository,
            authorization_provider,
        }
    }

    /// Create a new organization with generated ID
    #[instrument(skip(self, request), fields(name = %request.name, user_id = %request.user_id))]
    pub async fn create_organization(
        &self,
        request: CreateOrganizationRequest,
    ) -> DomainResult<Organization> {
        // Validate request using garde
        common::garde::validate(&request)?;

        debug!(name = %request.name, user_id = %request.user_id, "creating organization");

        // Generate unique organization ID using xid
        let organization_id = xid::new().to_string();

        // Save user_id before moving into repo_input
        let user_id = request.user_id.clone();

        let repo_input = CreateOrganizationRepoInputWithId {
            id: organization_id.clone(),
            name: request.name,
            user_id: request.user_id,
        };

        let organization = self.repository.create_organization(repo_input).await?;

        // Assign the creator as Admin in the authorization system
        self.authorization_provider
            .assign_role(&user_id, &organization.id, OrgRole::Admin)
            .await?;

        debug!(organization_id = %organization.id, "organization created successfully");
        Ok(organization)
    }

    /// Get organization by ID (excludes soft deleted)
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn get_organization(
        &self,
        request: GetOrganizationRequest,
    ) -> DomainResult<Organization> {
        // Validate request using garde
        common::garde::validate(&request)?;

        debug!(organization_id = %request.organization_id, "getting organization");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Organization,
                Action::Read,
            )
            .await?;

        let repo_input = GetOrganizationRepoInput {
            organization_id: request.organization_id.clone(),
        };

        let organization = self
            .repository
            .get_organization(repo_input)
            .await?
            .ok_or_else(|| DomainError::OrganizationNotFound(request.organization_id.clone()))?;

        Ok(organization)
    }

    /// Update organization name
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn update_organization(
        &self,
        request: UpdateOrganizationRequest,
    ) -> DomainResult<Organization> {
        // Validate request using garde
        common::garde::validate(&request)?;

        debug!(organization_id = %request.organization_id, "updating organization");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Organization,
                Action::Update,
            )
            .await?;

        let repo_input = UpdateOrganizationRepoInput {
            organization_id: request.organization_id,
            name: request.name,
        };

        let organization = self.repository.update_organization(repo_input).await?;

        debug!(organization_id = %organization.id, "organization updated successfully");
        Ok(organization)
    }

    /// Soft delete organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn delete_organization(
        &self,
        request: DeleteOrganizationRequest,
    ) -> DomainResult<()> {
        // Validate request using garde
        common::garde::validate(&request)?;

        debug!(organization_id = %request.organization_id, "Deleting organization");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Organization,
                Action::Delete,
            )
            .await?;

        let repo_input = DeleteOrganizationRepoInput {
            organization_id: request.organization_id.clone(),
        };

        self.repository.delete_organization(repo_input).await?;

        debug!(organization_id = %request.organization_id, "Organization soft deleted successfully");
        Ok(())
    }

    /// List all active organizations (excludes soft deleted)
    #[instrument(skip(self, _request))]
    pub async fn list_organizations(
        &self,
        _request: ListOrganizationsRequest,
    ) -> DomainResult<Vec<Organization>> {
        debug!("Listing organizations");

        let repo_input = ListOrganizationsRepoInput {};
        let organizations = self.repository.list_organizations(repo_input).await?;

        debug!(count = organizations.len(), "Listed organizations");
        Ok(organizations)
    }

    /// Get organizations that a user belongs to (excludes soft deleted)
    #[instrument(skip(self, request), fields(user_id = %request.user_id))]
    pub async fn get_user_organizations(
        &self,
        request: GetUserOrganizationsRequest,
    ) -> DomainResult<Vec<Organization>> {
        // Validate request using garde
        common::garde::validate(&request)?;

        debug!(user_id = %request.user_id, "getting organizations for user");

        let repo_input = GetUserOrganizationsRepoInput {
            user_id: request.user_id,
        };

        let organizations = self
            .repository
            .get_organizations_by_user_id(repo_input)
            .await?;

        debug!(count = organizations.len(), "found organizations for user");
        Ok(organizations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::MockOrganizationRepository;

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_assign_role()
            .returning(|_, _, _| Box::pin(async { Ok(()) }));
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

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
            .withf(|input: &CreateOrganizationRepoInputWithId| {
                !input.id.is_empty() && input.name == "Test Org" && input.user_id == "user-123"
            })
            .times(1)
            .return_once(move |_| Ok(expected_org.clone()));

        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());
        let request = CreateOrganizationRequest {
            name: "Test Org".to_string(),
            user_id: "user-123".to_string(),
        };

        let result = service.create_organization(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "Test Org");
    }

    #[tokio::test]
    async fn test_create_organization_empty_name() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = CreateOrganizationRequest {
            name: "".to_string(),
            user_id: "user-123".to_string(),
        };

        let result = service.create_organization(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_create_organization_empty_user_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = CreateOrganizationRequest {
            name: "Test Org".to_string(),
            user_id: "".to_string(),
        };

        let result = service.create_organization(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_get_organization_not_found() {
        let mut mock_repo = MockOrganizationRepository::new();

        mock_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());
        let request = GetOrganizationRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent".to_string(),
        };

        let result = service.get_organization(request).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_organization_empty_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = GetOrganizationRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "".to_string(),
        };

        let result = service.get_organization(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_update_organization_empty_name() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = UpdateOrganizationRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-123".to_string(),
            name: "".to_string(),
        };

        let result = service.update_organization(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_delete_organization_empty_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = DeleteOrganizationRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "".to_string(),
        };

        let result = service.delete_organization(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_get_user_organizations_empty_user_id() {
        let mock_repo = MockOrganizationRepository::new();
        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());

        let request = GetUserOrganizationsRequest {
            user_id: "".to_string(),
        };

        let result = service.get_user_organizations(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_get_user_organizations_success() {
        let mut mock_repo = MockOrganizationRepository::new();

        let expected_orgs = vec![
            Organization {
                id: "org-1".to_string(),
                name: "Org One".to_string(),
                deleted_at: None,
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            },
            Organization {
                id: "org-2".to_string(),
                name: "Org Two".to_string(),
                deleted_at: None,
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            },
        ];

        let cloned_orgs = expected_orgs.clone();
        mock_repo
            .expect_get_organizations_by_user_id()
            .withf(|input: &GetUserOrganizationsRepoInput| input.user_id == "user-123")
            .times(1)
            .return_once(move |_| Ok(cloned_orgs));

        let service = OrganizationService::new(Arc::new(mock_repo), create_mock_auth_provider());
        let request = GetUserOrganizationsRequest {
            user_id: "user-123".to_string(),
        };

        let result = service.get_user_organizations(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
