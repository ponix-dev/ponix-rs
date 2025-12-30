use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateWorkspaceRepoInputWithId, DeleteWorkspaceRepoInput, DomainError, DomainResult,
    GetOrganizationRepoInput, GetWorkspaceRepoInput, ListWorkspacesRepoInput,
    OrganizationRepository, UpdateWorkspaceRepoInput, Workspace, WorkspaceRepository,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

// ============================================================================
// Service Request Types
// ============================================================================
// These types are used by the service layer and embed user_id for authorization

/// Request to create a workspace
#[derive(Debug, Clone, Validate)]
pub struct CreateWorkspaceRequest {
    #[garde(length(min = 1))]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Request to get a workspace by ID
#[derive(Debug, Clone, Validate)]
pub struct GetWorkspaceRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
}

/// Request to update a workspace
#[derive(Debug, Clone, Validate)]
pub struct UpdateWorkspaceRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Request to delete a workspace
#[derive(Debug, Clone, Validate)]
pub struct DeleteWorkspaceRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
}

/// Request to list workspaces for an organization
#[derive(Debug, Clone, Validate)]
pub struct ListWorkspacesRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Domain service for workspace business logic
pub struct WorkspaceService {
    workspace_repository: Arc<dyn WorkspaceRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl WorkspaceService {
    pub fn new(
        workspace_repository: Arc<dyn WorkspaceRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            workspace_repository,
            organization_repository,
            authorization_provider,
        }
    }

    /// Validate that the organization exists and is not deleted
    async fn validate_organization_exists(&self, organization_id: &str) -> DomainResult<()> {
        let repo_input = GetOrganizationRepoInput {
            organization_id: organization_id.to_string(),
        };

        let organization = self
            .organization_repository
            .get_organization(repo_input)
            .await?;

        match organization {
            Some(org) if org.deleted_at.is_some() => {
                Err(DomainError::OrganizationDeleted(organization_id.to_string()))
            }
            Some(_) => Ok(()),
            None => Err(DomainError::OrganizationNotFound(
                organization_id.to_string(),
            )),
        }
    }

    /// Create a new workspace with generated ID
    #[instrument(skip(self, request), fields(name = %request.name, organization_id = %request.organization_id, user_id = %request.user_id))]
    pub async fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> DomainResult<Workspace> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(name = %request.name, organization_id = %request.organization_id, "creating workspace");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Workspace,
                Action::Create,
            )
            .await?;

        // Validate organization exists
        self.validate_organization_exists(&request.organization_id)
            .await?;

        // Generate unique workspace ID using xid
        let workspace_id = xid::new().to_string();

        let repo_input = CreateWorkspaceRepoInputWithId {
            id: workspace_id.clone(),
            name: request.name,
            organization_id: request.organization_id,
        };

        let workspace = self.workspace_repository.create_workspace(repo_input).await?;

        debug!(workspace_id = %workspace.id, "workspace created successfully");
        Ok(workspace)
    }

    /// Get workspace by ID (excludes soft deleted)
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id))]
    pub async fn get_workspace(&self, request: GetWorkspaceRequest) -> DomainResult<Workspace> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(workspace_id = %request.workspace_id, "getting workspace");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Workspace,
                Action::Read,
            )
            .await?;

        let repo_input = GetWorkspaceRepoInput {
            workspace_id: request.workspace_id.clone(),
            organization_id: request.organization_id,
        };

        let workspace = self
            .workspace_repository
            .get_workspace(repo_input)
            .await?
            .ok_or_else(|| DomainError::WorkspaceNotFound(request.workspace_id.clone()))?;

        Ok(workspace)
    }

    /// Update workspace name
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id))]
    pub async fn update_workspace(
        &self,
        request: UpdateWorkspaceRequest,
    ) -> DomainResult<Workspace> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(workspace_id = %request.workspace_id, "updating workspace");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Workspace,
                Action::Update,
            )
            .await?;

        let repo_input = UpdateWorkspaceRepoInput {
            workspace_id: request.workspace_id,
            organization_id: request.organization_id,
            name: request.name,
        };

        let workspace = self
            .workspace_repository
            .update_workspace(repo_input)
            .await?;

        debug!(workspace_id = %workspace.id, "workspace updated successfully");
        Ok(workspace)
    }

    /// Soft delete workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id))]
    pub async fn delete_workspace(&self, request: DeleteWorkspaceRequest) -> DomainResult<()> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(workspace_id = %request.workspace_id, "deleting workspace");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Workspace,
                Action::Delete,
            )
            .await?;

        let repo_input = DeleteWorkspaceRepoInput {
            workspace_id: request.workspace_id.clone(),
            organization_id: request.organization_id,
        };

        self.workspace_repository
            .delete_workspace(repo_input)
            .await?;

        debug!(workspace_id = %request.workspace_id, "workspace soft deleted successfully");
        Ok(())
    }

    /// List workspaces for an organization (excludes soft deleted)
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn list_workspaces(
        &self,
        request: ListWorkspacesRequest,
    ) -> DomainResult<Vec<Workspace>> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(organization_id = %request.organization_id, "listing workspaces");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Workspace,
                Action::Read,
            )
            .await?;

        let repo_input = ListWorkspacesRepoInput {
            organization_id: request.organization_id,
        };

        let workspaces = self
            .workspace_repository
            .list_workspaces(repo_input)
            .await?;

        debug!(count = workspaces.len(), "listed workspaces");
        Ok(workspaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{MockOrganizationRepository, MockWorkspaceRepository, Organization};

    const TEST_USER_ID: &str = "user-123";
    const TEST_ORG_ID: &str = "org-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn create_mock_org_repository() -> Arc<MockOrganizationRepository> {
        let mut mock = MockOrganizationRepository::new();
        mock.expect_get_organization().returning(|_| {
            Ok(Some(Organization {
                id: TEST_ORG_ID.to_string(),
                name: "Test Org".to_string(),
                deleted_at: None,
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            }))
        });
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_create_workspace_success() {
        let mut mock_ws_repo = MockWorkspaceRepository::new();

        let expected_ws = Workspace {
            id: "generated-id".to_string(),
            name: "Test Workspace".to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            deleted_at: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_ws_repo
            .expect_create_workspace()
            .withf(|input: &CreateWorkspaceRepoInputWithId| {
                !input.id.is_empty()
                    && input.name == "Test Workspace"
                    && input.organization_id == TEST_ORG_ID
            })
            .times(1)
            .return_once(move |_| Ok(expected_ws.clone()));

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = CreateWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "Test Workspace".to_string(),
        };

        let result = service.create_workspace(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "Test Workspace");
    }

    #[tokio::test]
    async fn test_create_workspace_empty_name() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = CreateWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            name: "".to_string(),
        };

        let result = service.create_workspace(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_create_workspace_empty_organization_id() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = CreateWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "".to_string(),
            name: "Test Workspace".to_string(),
        };

        let result = service.create_workspace(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_create_workspace_organization_not_found() {
        let mock_ws_repo = MockWorkspaceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .returning(|_| Ok(None));

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent".to_string(),
            name: "Test Workspace".to_string(),
        };

        let result = service.create_workspace(request).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_workspace_not_found() {
        let mut mock_ws_repo = MockWorkspaceRepository::new();

        mock_ws_repo
            .expect_get_workspace()
            .times(1)
            .return_once(|_| Ok(None));

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = GetWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            workspace_id: "nonexistent".to_string(),
        };

        let result = service.get_workspace(request).await;
        assert!(matches!(result, Err(DomainError::WorkspaceNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_workspace_empty_id() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = GetWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            workspace_id: "".to_string(),
        };

        let result = service.get_workspace(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_update_workspace_empty_name() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = UpdateWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            workspace_id: "ws-123".to_string(),
            name: "".to_string(),
        };

        let result = service.update_workspace(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_delete_workspace_empty_id() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = DeleteWorkspaceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
            workspace_id: "".to_string(),
        };

        let result = service.delete_workspace(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_list_workspaces_success() {
        let mut mock_ws_repo = MockWorkspaceRepository::new();

        let expected_workspaces = vec![
            Workspace {
                id: "ws-1".to_string(),
                name: "Workspace One".to_string(),
                organization_id: TEST_ORG_ID.to_string(),
                deleted_at: None,
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            },
            Workspace {
                id: "ws-2".to_string(),
                name: "Workspace Two".to_string(),
                organization_id: TEST_ORG_ID.to_string(),
                deleted_at: None,
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            },
        ];

        let cloned_workspaces = expected_workspaces.clone();
        mock_ws_repo
            .expect_list_workspaces()
            .withf(|input: &ListWorkspacesRepoInput| input.organization_id == TEST_ORG_ID)
            .times(1)
            .return_once(move |_| Ok(cloned_workspaces));

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = ListWorkspacesRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: TEST_ORG_ID.to_string(),
        };

        let result = service.list_workspaces(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_list_workspaces_empty_organization_id() {
        let mock_ws_repo = MockWorkspaceRepository::new();

        let service = WorkspaceService::new(
            Arc::new(mock_ws_repo),
            create_mock_org_repository(),
            create_mock_auth_provider(),
        );

        let request = ListWorkspacesRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "".to_string(),
        };

        let result = service.list_workspaces(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }
}
