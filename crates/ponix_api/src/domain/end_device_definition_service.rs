use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateEndDeviceDefinitionRepoInput, DeleteEndDeviceDefinitionRepoInput, DomainError,
    DomainResult, EndDeviceDefinition, EndDeviceDefinitionRepository,
    GetEndDeviceDefinitionRepoInput, GetOrganizationRepoInput,
    ListEndDeviceDefinitionsRepoInput, OrganizationRepository,
    UpdateEndDeviceDefinitionRepoInput,
};
use common::jsonschema::validate_json_schema;
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a definition
#[derive(Debug, Clone, Validate)]
pub struct CreateEndDeviceDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(skip)] // Validated separately with JSON Schema validator
    pub json_schema: String,
    #[garde(skip)] // payload_conversion can be empty
    pub payload_conversion: String,
}

/// Service request for getting a definition
#[derive(Debug, Clone, Validate)]
pub struct GetEndDeviceDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for updating a definition
#[derive(Debug, Clone, Validate)]
pub struct UpdateEndDeviceDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(inner(length(min = 1)))]
    pub name: Option<String>,
    #[garde(skip)] // Validated separately with JSON Schema validator
    pub json_schema: Option<String>,
    #[garde(skip)] // payload_conversion can be empty
    pub payload_conversion: Option<String>,
}

/// Service request for deleting a definition
#[derive(Debug, Clone, Validate)]
pub struct DeleteEndDeviceDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing definitions
#[derive(Debug, Clone, Validate)]
pub struct ListEndDeviceDefinitionsRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Domain service for end device definition management
pub struct EndDeviceDefinitionService {
    definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl EndDeviceDefinitionService {
    pub fn new(
        definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            definition_repository,
            organization_repository,
            authorization_provider,
        }
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, name = %request.name))]
    pub async fn create_definition(
        &self,
        request: CreateEndDeviceDefinitionRequest,
    ) -> DomainResult<EndDeviceDefinition> {
        common::garde::validate(&request)?;

        // Validate JSON Schema syntax
        validate_json_schema(&request.json_schema)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device, // Using Device resource for now
                Action::Create,
            )
            .await?;

        // Validate organization exists and is not deleted
        self.validate_organization(&request.organization_id).await?;

        // Generate unique ID
        let id = xid::new().to_string();

        debug!(id = %id, "creating end device definition");

        let repo_input = CreateEndDeviceDefinitionRepoInput {
            id,
            organization_id: request.organization_id,
            name: request.name,
            json_schema: request.json_schema,
            payload_conversion: request.payload_conversion,
        };

        self.definition_repository
            .create_definition(repo_input)
            .await
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn get_definition(
        &self,
        request: GetEndDeviceDefinitionRequest,
    ) -> DomainResult<EndDeviceDefinition> {
        common::garde::validate(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Read,
            )
            .await?;

        let repo_input = GetEndDeviceDefinitionRepoInput {
            id: request.id.clone(),
            organization_id: request.organization_id,
        };

        self.definition_repository
            .get_definition(repo_input)
            .await?
            .ok_or_else(|| DomainError::EndDeviceDefinitionNotFound(request.id))
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn update_definition(
        &self,
        request: UpdateEndDeviceDefinitionRequest,
    ) -> DomainResult<EndDeviceDefinition> {
        common::garde::validate(&request)?;

        // Validate JSON Schema if provided
        if let Some(ref schema) = request.json_schema {
            validate_json_schema(schema)?;
        }

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Update,
            )
            .await?;

        let repo_input = UpdateEndDeviceDefinitionRepoInput {
            id: request.id,
            organization_id: request.organization_id,
            name: request.name,
            json_schema: request.json_schema,
            payload_conversion: request.payload_conversion,
        };

        self.definition_repository
            .update_definition(repo_input)
            .await
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn delete_definition(
        &self,
        request: DeleteEndDeviceDefinitionRequest,
    ) -> DomainResult<()> {
        common::garde::validate(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Delete,
            )
            .await?;

        let repo_input = DeleteEndDeviceDefinitionRepoInput {
            id: request.id,
            organization_id: request.organization_id,
        };

        self.definition_repository
            .delete_definition(repo_input)
            .await
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn list_definitions(
        &self,
        request: ListEndDeviceDefinitionsRequest,
    ) -> DomainResult<Vec<EndDeviceDefinition>> {
        common::garde::validate(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Read,
            )
            .await?;

        let repo_input = ListEndDeviceDefinitionsRepoInput {
            organization_id: request.organization_id,
        };

        self.definition_repository
            .list_definitions(repo_input)
            .await
    }

    async fn validate_organization(&self, organization_id: &str) -> DomainResult<()> {
        let org_input = GetOrganizationRepoInput {
            organization_id: organization_id.to_string(),
        };

        match self
            .organization_repository
            .get_organization(org_input)
            .await?
        {
            Some(org) => {
                if org.deleted_at.is_some() {
                    return Err(DomainError::OrganizationDeleted(format!(
                        "Cannot create definition for deleted organization: {}",
                        organization_id
                    )));
                }
                Ok(())
            }
            None => Err(DomainError::OrganizationNotFound(format!(
                "Organization not found: {}",
                organization_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{MockEndDeviceDefinitionRepository, MockOrganizationRepository, Organization};

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_create_definition_success() {
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        // Mock organization exists
        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        let expected_def = EndDeviceDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            json_schema: r#"{"type": "object"}"#.to_string(),
            payload_conversion: "cayenne_lpp.decode(payload)".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_create_definition()
            .times(1)
            .return_once(move |_| Ok(expected_def));

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            json_schema: r#"{"type": "object"}"#.to_string(),
            payload_conversion: "cayenne_lpp.decode(payload)".to_string(),
        };

        let result = service.create_definition(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_definition_invalid_json_schema() {
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            json_schema: "not valid json".to_string(),
            payload_conversion: "".to_string(),
        };

        let result = service.create_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::InvalidJsonSchema(_)
        ));
    }

    #[tokio::test]
    async fn test_create_definition_empty_name() {
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "".to_string(),
            json_schema: "{}".to_string(),
            payload_conversion: "".to_string(),
        };

        let result = service.create_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_get_definition_success() {
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let expected_def = EndDeviceDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            json_schema: "{}".to_string(),
            payload_conversion: "".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(move |_| Ok(Some(expected_def)));

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = GetEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_definition(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_definition_not_found() {
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = GetEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "nonexistent".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::EndDeviceDefinitionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_definitions_success() {
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let definitions = vec![
            EndDeviceDefinition {
                id: "def-1".to_string(),
                organization_id: "org-456".to_string(),
                name: "Definition 1".to_string(),
                json_schema: "{}".to_string(),
                payload_conversion: "".to_string(),
                created_at: None,
                updated_at: None,
            },
            EndDeviceDefinition {
                id: "def-2".to_string(),
                organization_id: "org-456".to_string(),
                name: "Definition 2".to_string(),
                json_schema: "{}".to_string(),
                payload_conversion: "".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_def_repo
            .expect_list_definitions()
            .times(1)
            .return_once(move |_| Ok(definitions));

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = ListEndDeviceDefinitionsRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.list_definitions(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_update_definition_with_invalid_schema() {
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = EndDeviceDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UpdateEndDeviceDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: None,
            json_schema: Some("invalid json".to_string()),
            payload_conversion: None,
        };

        let result = service.update_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::InvalidJsonSchema(_)
        ));
    }
}
