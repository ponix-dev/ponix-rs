use common::auth::{Action, AuthorizationProvider, Resource};
use common::cel::CelExpressionCompiler;
use common::domain::{
    CreateDataStreamDefinitionRepoInput, DataStreamDefinition, DataStreamDefinitionRepository,
    DeleteDataStreamDefinitionRepoInput, DomainError, DomainResult,
    GetDataStreamDefinitionRepoInput, GetOrganizationRepoInput, ListDataStreamDefinitionsRepoInput,
    OrganizationRepository, PayloadContract, UpdateDataStreamDefinitionRepoInput,
};
use common::jsonschema::validate_json_schema;
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a definition
#[derive(Debug, Clone, Validate)]
pub struct CreateDataStreamDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub contracts: Vec<PayloadContract>,
}

/// Service request for getting a definition
#[derive(Debug, Clone, Validate)]
pub struct GetDataStreamDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for updating a definition
#[derive(Debug, Clone, Validate)]
pub struct UpdateDataStreamDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(inner(length(min = 1)))]
    pub name: Option<String>,
    #[garde(skip)] // Contracts validated separately
    pub contracts: Option<Vec<PayloadContract>>,
}

/// Service request for deleting a definition
#[derive(Debug, Clone, Validate)]
pub struct DeleteDataStreamDefinitionRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing definitions
#[derive(Debug, Clone, Validate)]
pub struct ListDataStreamDefinitionsRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Domain service for data stream definition management
pub struct DataStreamDefinitionService {
    definition_repository: Arc<dyn DataStreamDefinitionRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
    cel_compiler: Arc<dyn CelExpressionCompiler>,
}

impl DataStreamDefinitionService {
    pub fn new(
        definition_repository: Arc<dyn DataStreamDefinitionRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
        cel_compiler: Arc<dyn CelExpressionCompiler>,
    ) -> Self {
        Self {
            definition_repository,
            organization_repository,
            authorization_provider,
            cel_compiler,
        }
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, name = %request.name))]
    pub async fn create_definition(
        &self,
        request: CreateDataStreamDefinitionRequest,
    ) -> DomainResult<DataStreamDefinition> {
        common::garde::validate_struct(&request)?;

        // Validate each contract's JSON Schema syntax
        for contract in &request.contracts {
            validate_json_schema(&contract.json_schema)?;
        }

        // Compile CEL expressions for each contract
        let contracts = self.compile_contracts(request.contracts)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Create,
            )
            .await?;

        // Validate organization exists and is not deleted
        self.validate_organization(&request.organization_id).await?;

        // Generate unique ID
        let id = xid::new().to_string();

        debug!(id = %id, "creating data stream definition");

        let repo_input = CreateDataStreamDefinitionRepoInput {
            id,
            organization_id: request.organization_id,
            name: request.name,
            contracts,
        };

        self.definition_repository
            .create_definition(repo_input)
            .await
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn get_definition(
        &self,
        request: GetDataStreamDefinitionRequest,
    ) -> DomainResult<DataStreamDefinition> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Read,
            )
            .await?;

        let repo_input = GetDataStreamDefinitionRepoInput {
            id: request.id.clone(),
            organization_id: request.organization_id,
        };

        self.definition_repository
            .get_definition(repo_input)
            .await?
            .ok_or_else(|| DomainError::DataStreamDefinitionNotFound(request.id))
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn update_definition(
        &self,
        request: UpdateDataStreamDefinitionRequest,
    ) -> DomainResult<DataStreamDefinition> {
        common::garde::validate_struct(&request)?;

        // Validate each contract's JSON Schema if contracts provided
        if let Some(ref contracts) = request.contracts {
            for contract in contracts {
                validate_json_schema(&contract.json_schema)?;
            }
        }

        // Compile CEL expressions if contracts provided
        let contracts = match request.contracts {
            Some(contracts) => Some(self.compile_contracts(contracts)?),
            None => None,
        };

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Update,
            )
            .await?;

        let repo_input = UpdateDataStreamDefinitionRepoInput {
            id: request.id,
            organization_id: request.organization_id,
            name: request.name,
            contracts,
        };

        self.definition_repository
            .update_definition(repo_input)
            .await
    }

    #[instrument(skip(self, request), fields(user_id = %request.user_id, id = %request.id, organization_id = %request.organization_id))]
    pub async fn delete_definition(
        &self,
        request: DeleteDataStreamDefinitionRequest,
    ) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Delete,
            )
            .await?;

        let repo_input = DeleteDataStreamDefinitionRepoInput {
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
        request: ListDataStreamDefinitionsRequest,
    ) -> DomainResult<Vec<DataStreamDefinition>> {
        common::garde::validate_struct(&request)?;

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Read,
            )
            .await?;

        let repo_input = ListDataStreamDefinitionsRepoInput {
            organization_id: request.organization_id,
        };

        self.definition_repository
            .list_definitions(repo_input)
            .await
    }

    fn compile_contracts(
        &self,
        contracts: Vec<PayloadContract>,
    ) -> DomainResult<Vec<PayloadContract>> {
        contracts
            .into_iter()
            .map(|mut contract| {
                contract.compiled_match = self.cel_compiler.compile(&contract.match_expression)?;
                contract.compiled_transform =
                    self.cel_compiler.compile(&contract.transform_expression)?;
                Ok(contract)
            })
            .collect()
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
    use common::cel::MockCelExpressionCompiler;
    use common::domain::{
        MockDataStreamDefinitionRepository, MockOrganizationRepository, Organization,
    };

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn create_mock_cel_compiler() -> Arc<MockCelExpressionCompiler> {
        let mut mock = MockCelExpressionCompiler::new();
        mock.expect_compile()
            .returning(|_| Ok(vec![0x01, 0x02, 0x03]));
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_create_definition_success() {
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

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

        let expected_def = DataStreamDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: r#"{"type": "object"}"#.to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_create_definition()
            .times(1)
            .return_once(move |_| Ok(expected_def));

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = CreateDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: r#"{"type": "object"}"#.to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
        };

        let result = service.create_definition(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_definition_invalid_json_schema_in_contract() {
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = CreateDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: "not valid json".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
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
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = CreateDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            name: "".to_string(),
            contracts: vec![],
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
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let expected_def = DataStreamDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![],
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(move |_| Ok(Some(expected_def)));

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = GetDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_definition(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_definition_not_found() {
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = GetDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "nonexistent".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::DataStreamDefinitionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_definitions_success() {
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let definitions = vec![
            DataStreamDefinition {
                id: "def-1".to_string(),
                organization_id: "org-456".to_string(),
                name: "Definition 1".to_string(),
                contracts: vec![],
                created_at: None,
                updated_at: None,
            },
            DataStreamDefinition {
                id: "def-2".to_string(),
                organization_id: "org-456".to_string(),
                name: "Definition 2".to_string(),
                contracts: vec![],
                created_at: None,
                updated_at: None,
            },
        ];

        mock_def_repo
            .expect_list_definitions()
            .times(1)
            .return_once(move |_| Ok(definitions));

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = ListDataStreamDefinitionsRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.list_definitions(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_update_definition_with_invalid_schema_in_contract() {
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = DataStreamDefinitionService::new(
            Arc::new(mock_def_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
            create_mock_cel_compiler(),
        );

        let request = UpdateDataStreamDefinitionRequest {
            user_id: TEST_USER_ID.to_string(),
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: None,
            contracts: Some(vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: "invalid json".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }]),
        };

        let result = service.update_definition(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::InvalidJsonSchema(_)
        ));
    }
}
