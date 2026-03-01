use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateDataStreamRepoInput, DataStream, DataStreamDefinitionRepository, DataStreamRepository,
    DomainError, DomainResult, GatewayRepository, GetDataStreamDefinitionRepoInput,
    GetDataStreamRepoInput, GetGatewayRepoInput, GetOrganizationRepoInput,
    ListDataStreamsByGatewayRepoInput, ListDataStreamsRepoInput, OrganizationRepository,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a data stream
#[derive(Debug, Clone, Validate)]
pub struct CreateDataStreamRequest {
    #[garde(skip)] // user_id validated by auth layer
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
    #[garde(length(min = 1))]
    pub definition_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Service request for getting a data stream
#[derive(Debug, Clone, Validate)]
pub struct GetDataStreamRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub data_stream_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
}

/// Service request for listing data streams in a workspace
#[derive(Debug, Clone, Validate)]
pub struct ListDataStreamsRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub workspace_id: String,
}

/// Service request for listing data streams by gateway
#[derive(Debug, Clone, Validate)]
pub struct ListDataStreamsByGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
}

/// Domain service for data stream management business logic
/// This is the orchestration layer that handlers call
pub struct DataStreamService {
    data_stream_repository: Arc<dyn DataStreamRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    definition_repository: Arc<dyn DataStreamDefinitionRepository>,
    gateway_repository: Arc<dyn GatewayRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl DataStreamService {
    pub fn new(
        data_stream_repository: Arc<dyn DataStreamRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        definition_repository: Arc<dyn DataStreamDefinitionRepository>,
        gateway_repository: Arc<dyn GatewayRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            data_stream_repository,
            organization_repository,
            definition_repository,
            gateway_repository,
            authorization_provider,
        }
    }

    /// Create a new data stream with business logic validation
    /// Generates a unique data_stream_id using xid
    /// Validates that the organization exists and is not deleted
    /// Validates that the definition exists and belongs to the same organization
    /// Validates that the gateway exists and belongs to the same organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id, data_stream_name = %request.name))]
    pub async fn create_data_stream(
        &self,
        request: CreateDataStreamRequest,
    ) -> DomainResult<DataStream> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

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
        debug!(organization_id = %request.organization_id, "validating organization exists");
        let org_input = GetOrganizationRepoInput {
            organization_id: request.organization_id.clone(),
        };

        match self
            .organization_repository
            .get_organization(org_input)
            .await?
        {
            Some(org) => {
                if org.deleted_at.is_some() {
                    return Err(DomainError::OrganizationDeleted(format!(
                        "Cannot create data stream for deleted organization: {}",
                        request.organization_id
                    )));
                }
            }
            None => {
                return Err(DomainError::OrganizationNotFound(format!(
                    "Organization not found: {}",
                    request.organization_id
                )));
            }
        }

        // Validate definition exists and belongs to the same organization
        debug!(definition_id = %request.definition_id, "validating definition exists");
        let def_input = GetDataStreamDefinitionRepoInput {
            id: request.definition_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.definition_repository
            .get_definition(def_input)
            .await?
            .ok_or_else(|| {
                DomainError::DataStreamDefinitionNotFound(request.definition_id.clone())
            })?;

        // Validate gateway exists and belongs to the same organization
        debug!(gateway_id = %request.gateway_id, "validating gateway exists");
        let gateway_input = GetGatewayRepoInput {
            gateway_id: request.gateway_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.gateway_repository
            .get_gateway(gateway_input)
            .await?
            .ok_or_else(|| {
                DomainError::GatewayNotFound(format!(
                    "Gateway {} not found in organization {}",
                    request.gateway_id, request.organization_id
                ))
            })?;

        // Generate unique data stream ID
        let data_stream_id = xid::new().to_string();

        debug!(data_stream_id = %data_stream_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id, "creating data stream");

        // Create input with generated ID for repository
        let repo_input = CreateDataStreamRepoInput {
            data_stream_id,
            organization_id: request.organization_id,
            workspace_id: request.workspace_id,
            definition_id: request.definition_id,
            gateway_id: request.gateway_id,
            name: request.name,
        };

        let data_stream = self
            .data_stream_repository
            .create_data_stream(repo_input)
            .await?;

        Ok(data_stream)
    }

    /// Get a data stream by ID, organization, and workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, data_stream_id = %request.data_stream_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id))]
    pub async fn get_data_stream(&self, request: GetDataStreamRequest) -> DomainResult<DataStream> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Read,
            )
            .await?;

        debug!(data_stream_id = %request.data_stream_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id, "getting data stream");

        let repo_input = GetDataStreamRepoInput {
            data_stream_id: request.data_stream_id,
            organization_id: request.organization_id,
            workspace_id: request.workspace_id,
        };

        let data_stream = self
            .data_stream_repository
            .get_data_stream(repo_input)
            .await?
            .ok_or_else(|| DomainError::DataStreamNotFound("Data stream not found".to_string()))?;

        Ok(data_stream)
    }

    /// List data streams for a workspace
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, workspace_id = %request.workspace_id))]
    pub async fn list_data_streams(
        &self,
        request: ListDataStreamsRequest,
    ) -> DomainResult<Vec<DataStream>> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization (at org level)
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Read,
            )
            .await?;

        debug!(organization_id = %request.organization_id, workspace_id = %request.workspace_id, "listing data streams for workspace");

        let repo_input = ListDataStreamsRepoInput {
            organization_id: request.organization_id,
            workspace_id: request.workspace_id,
        };

        let data_streams = self
            .data_stream_repository
            .list_data_streams(repo_input)
            .await?;

        debug!(
            count = data_streams.len(),
            "listed data streams for workspace"
        );
        Ok(data_streams)
    }

    /// List data streams for a gateway
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id))]
    pub async fn list_data_streams_by_gateway(
        &self,
        request: ListDataStreamsByGatewayRequest,
    ) -> DomainResult<Vec<DataStream>> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization (at org level for data stream read)
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::DataStream,
                Action::Read,
            )
            .await?;

        // Validate gateway exists and belongs to the organization
        debug!(gateway_id = %request.gateway_id, "validating gateway exists");
        let gateway_input = GetGatewayRepoInput {
            gateway_id: request.gateway_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.gateway_repository
            .get_gateway(gateway_input)
            .await?
            .ok_or_else(|| {
                DomainError::GatewayNotFound(format!(
                    "Gateway {} not found in organization {}",
                    request.gateway_id, request.organization_id
                ))
            })?;

        debug!(organization_id = %request.organization_id, gateway_id = %request.gateway_id, "listing data streams for gateway");

        let repo_input = ListDataStreamsByGatewayRepoInput {
            organization_id: request.organization_id,
            gateway_id: request.gateway_id,
        };

        let data_streams = self
            .data_stream_repository
            .list_data_streams_by_gateway(repo_input)
            .await?;

        debug!(
            count = data_streams.len(),
            "listed data streams for gateway"
        );
        Ok(data_streams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        DataStreamDefinition, Gateway, GatewayConfig, MockDataStreamDefinitionRepository,
        MockDataStreamRepository, MockGatewayRepository, MockOrganizationRepository, Organization,
    };

    const TEST_USER_ID: &str = "user-123";
    const TEST_GATEWAY_ID: &str = "gw-001";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn create_mock_gateway_repo() -> MockGatewayRepository {
        let mut mock = MockGatewayRepository::new();
        mock.expect_get_gateway().returning(|input| {
            Ok(Some(Gateway {
                gateway_id: input.gateway_id.clone(),
                organization_id: input.organization_id.clone(),
                name: "Test Gateway".to_string(),
                gateway_type: "emqx".to_string(),
                gateway_config: GatewayConfig::Emqx(common::domain::EmqxGatewayConfig {
                    broker_url: "mqtt://localhost:1883".to_string(),
                }),
                deleted_at: None,
                created_at: None,
                updated_at: None,
            }))
        });
        mock
    }

    #[tokio::test]
    async fn test_create_data_stream_success() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = create_mock_gateway_repo();

        // Mock organization exists and is active
        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        // Mock definition exists
        let def = DataStreamDefinition {
            id: "def-789".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![],
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_get_definition()
            .withf(|input: &GetDataStreamDefinitionRepoInput| {
                input.id == "def-789" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(def)));

        let expected_data_stream = DataStream {
            data_stream_id: "ds-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_data_stream_repo
            .expect_create_data_stream()
            .withf(|input: &CreateDataStreamRepoInput| {
                !input.data_stream_id.is_empty() // ID is generated
                    && input.organization_id == "org-456"
                    && input.workspace_id == "ws-123"
                    && input.definition_id == "def-789"
                    && input.gateway_id == TEST_GATEWAY_ID
                    && input.name == "Test Data Stream"
            })
            .times(1)
            .return_once(move |_| Ok(expected_data_stream.clone()));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_ok());

        let data_stream = result.unwrap();
        assert!(!data_stream.data_stream_id.is_empty()); // ID was generated
        assert_eq!(data_stream.name, "Test Data Stream");
        assert_eq!(data_stream.definition_id, "def-789");
        assert_eq!(data_stream.gateway_id, TEST_GATEWAY_ID);
    }

    #[tokio::test]
    async fn test_create_data_stream_empty_name() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_create_data_stream_organization_not_found() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        // Mock organization not found
        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent-org".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_create_data_stream_organization_deleted() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        // Mock deleted organization
        let deleted_org = Organization {
            id: "org-deleted".to_string(),
            name: "Deleted Org".to_string(),
            deleted_at: Some(chrono::Utc::now()),
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(deleted_org)));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-deleted".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationDeleted(_)
        ));
    }

    #[tokio::test]
    async fn test_create_data_stream_definition_not_found() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

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

        // Mock definition not found
        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "nonexistent-def".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::DataStreamDefinitionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_data_stream_success() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let expected_data_stream = DataStream {
            data_stream_id: "ds-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_data_stream_repo
            .expect_get_data_stream()
            .withf(|input: &GetDataStreamRepoInput| {
                input.data_stream_id == "ds-123"
                    && input.organization_id == "org-456"
                    && input.workspace_id == "ws-123"
            })
            .times(1)
            .return_once(move |_| Ok(Some(expected_data_stream)));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            data_stream_id: "ds-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
        };

        let result = service.get_data_stream(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_data_stream_not_found() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        mock_data_stream_repo
            .expect_get_data_stream()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            data_stream_id: "nonexistent".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
        };

        let result = service.get_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::DataStreamNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_data_stream_empty_organization_id_fails() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            data_stream_id: "ds-123".to_string(),
            organization_id: "".to_string(),
            workspace_id: "ws-123".to_string(),
        };

        let result = service.get_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_list_data_streams_success() {
        let mut mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let data_streams = vec![
            DataStream {
                data_stream_id: "ds-1".to_string(),
                organization_id: "org-456".to_string(),
                workspace_id: "ws-123".to_string(),
                definition_id: "def-789".to_string(),
                gateway_id: TEST_GATEWAY_ID.to_string(),
                name: "Data Stream 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            DataStream {
                data_stream_id: "ds-2".to_string(),
                organization_id: "org-456".to_string(),
                workspace_id: "ws-123".to_string(),
                definition_id: "def-789".to_string(),
                gateway_id: TEST_GATEWAY_ID.to_string(),
                name: "Data Stream 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_data_stream_repo
            .expect_list_data_streams()
            .withf(|input: &ListDataStreamsRepoInput| {
                input.organization_id == "org-456" && input.workspace_id == "ws-123"
            })
            .times(1)
            .return_once(move |_| Ok(data_streams));

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = ListDataStreamsRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
        };

        let result = service.list_data_streams(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_create_data_stream_permission_denied() {
        let mock_data_stream_repo = MockDataStreamRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockDataStreamDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let mut mock_auth = MockAuthorizationProvider::new();
        mock_auth
            .expect_require_permission()
            .returning(|_, _, _, _| {
                Box::pin(async {
                    Err(DomainError::PermissionDenied(
                        "User does not have permission".to_string(),
                    ))
                })
            });

        let service = DataStreamService::new(
            Arc::new(mock_data_stream_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            Arc::new(mock_auth),
        );

        let request = CreateDataStreamRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-123".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test Data Stream".to_string(),
        };

        let result = service.create_data_stream(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::PermissionDenied(_)
        ));
    }
}
