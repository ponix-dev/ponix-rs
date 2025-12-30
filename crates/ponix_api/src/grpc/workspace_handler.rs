use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, instrument};

use crate::domain::{
    CreateWorkspaceRequest as DomainCreateRequest, DeleteWorkspaceRequest as DomainDeleteRequest,
    GetWorkspaceRequest as DomainGetRequest, ListWorkspacesRequest as DomainListRequest,
    UpdateWorkspaceRequest as DomainUpdateRequest, WorkspaceService,
};
use common::auth::AuthTokenProvider;
use common::grpc::{domain_error_to_status, extract_user_context};
use common::proto::to_proto_workspace;
use ponix_proto_prost::workspace::v1::{
    CreateWorkspaceRequest, CreateWorkspaceResponse, DeleteWorkspaceRequest,
    DeleteWorkspaceResponse, GetWorkspaceRequest, GetWorkspaceResponse, ListWorkspacesRequest,
    ListWorkspacesResponse, UpdateWorkspaceRequest, UpdateWorkspaceResponse,
};
use ponix_proto_tonic::workspace::v1::tonic::workspace_service_server::WorkspaceService as WorkspaceServiceTrait;

/// gRPC handler for WorkspaceService
/// Handles Proto → Domain mapping and error conversion
pub struct WorkspaceServiceHandler {
    domain_service: Arc<WorkspaceService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
}

impl WorkspaceServiceHandler {
    pub fn new(
        domain_service: Arc<WorkspaceService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
    ) -> Self {
        Self {
            domain_service,
            auth_token_provider,
        }
    }
}

#[tonic::async_trait]
impl WorkspaceServiceTrait for WorkspaceServiceHandler {
    #[instrument(
        name = "CreateWorkspace",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, workspace_name = %request.get_ref().name)
    )]
    async fn create_workspace(
        &self,
        request: Request<CreateWorkspaceRequest>,
    ) -> Result<Response<CreateWorkspaceResponse>, Status> {
        // Extract user_id from authorization header (mandatory)
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;

        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainCreateRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            name: req.name,
        };

        // Call domain service
        let workspace = self
            .domain_service
            .create_workspace(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(workspace_id = %workspace.id, "Workspace created successfully");

        // Convert to proto and return
        let proto_workspace = to_proto_workspace(workspace);

        Ok(Response::new(CreateWorkspaceResponse {
            workspace: Some(proto_workspace),
        }))
    }

    #[instrument(
        name = "GetWorkspace",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, workspace_id = %request.get_ref().workspace_id)
    )]
    async fn get_workspace(
        &self,
        request: Request<GetWorkspaceRequest>,
    ) -> Result<Response<GetWorkspaceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainGetRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        let workspace = self
            .domain_service
            .get_workspace(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_workspace = to_proto_workspace(workspace);

        Ok(Response::new(GetWorkspaceResponse {
            workspace: Some(proto_workspace),
        }))
    }

    #[instrument(
        name = "UpdateWorkspace",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, workspace_id = %request.get_ref().workspace_id)
    )]
    async fn update_workspace(
        &self,
        request: Request<UpdateWorkspaceRequest>,
    ) -> Result<Response<UpdateWorkspaceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainUpdateRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
            name: req.name,
        };

        // Call domain service
        let workspace = self
            .domain_service
            .update_workspace(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(workspace_id = %workspace.id, "Workspace updated successfully");

        // Convert domain → proto
        let proto_workspace = to_proto_workspace(workspace);

        Ok(Response::new(UpdateWorkspaceResponse {
            workspace: Some(proto_workspace),
        }))
    }

    #[instrument(
        name = "DeleteWorkspace",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id, workspace_id = %request.get_ref().workspace_id)
    )]
    async fn delete_workspace(
        &self,
        request: Request<DeleteWorkspaceRequest>,
    ) -> Result<Response<DeleteWorkspaceResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let workspace_id = req.workspace_id.clone();
        let service_request = DomainDeleteRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
            workspace_id: req.workspace_id,
        };

        // Call domain service
        self.domain_service
            .delete_workspace(service_request)
            .await
            .map_err(domain_error_to_status)?;

        debug!(workspace_id = %workspace_id, "Workspace deleted successfully");

        // Return empty response with workspace field (required by proto)
        Ok(Response::new(DeleteWorkspaceResponse { workspace: None }))
    }

    #[instrument(
        name = "ListWorkspaces",
        skip(self, request),
        fields(organization_id = %request.get_ref().organization_id)
    )]
    async fn list_workspaces(
        &self,
        request: Request<ListWorkspacesRequest>,
    ) -> Result<Response<ListWorkspacesResponse>, Status> {
        // Extract user context from JWT
        let user_context = extract_user_context(&request, self.auth_token_provider.as_ref())?;
        let req = request.into_inner();

        // Construct domain request directly
        let service_request = DomainListRequest {
            user_id: user_context.user_id,
            organization_id: req.organization_id,
        };

        // Call domain service
        let workspaces = self
            .domain_service
            .list_workspaces(service_request)
            .await
            .map_err(domain_error_to_status)?;

        // Convert domain → proto
        let proto_workspaces: Vec<_> = workspaces.into_iter().map(to_proto_workspace).collect();

        debug!(
            count = proto_workspaces.len(),
            "Workspaces listed successfully"
        );

        Ok(Response::new(ListWorkspacesResponse {
            workspaces: proto_workspaces,
        }))
    }
}
