//! gRPC server setup for Ponix API.
//!
//! This module builds the Routes for all Ponix API services and delegates
//! to the reusable `common::grpc::run_grpc_server` for actual server execution.

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::service::Routes;

use crate::domain::{
    DataStreamDefinitionService, DataStreamService, DocumentService, GatewayService,
    OrganizationService, UserService, WorkspaceService,
};
use crate::grpc::{
    DataStreamDefinitionServiceHandler, DataStreamServiceHandler, DocumentServiceHandler,
    GatewayServiceHandler, OrganizationServiceHandler, UserServiceHandler,
    WorkspaceServiceHandler,
};
use common::auth::AuthTokenProvider;
use common::grpc::{run_grpc_server, GrpcServerConfig};
use ponix_proto_prost;
use ponix_proto_tonic::data_stream::v1::tonic::data_stream_definition_service_server::DataStreamDefinitionServiceServer;
use ponix_proto_tonic::data_stream::v1::tonic::data_stream_service_server::DataStreamServiceServer;
use ponix_proto_tonic::gateway::v1::tonic::gateway_service_server::GatewayServiceServer;
use ponix_proto_tonic::organization::v1::tonic::organization_service_server::OrganizationServiceServer;
use ponix_proto_tonic::user::v1::tonic::user_service_server::UserServiceServer;
use ponix_proto_tonic::document::v1::tonic::document_service_server::DocumentServiceServer;
use ponix_proto_tonic::workspace::v1::tonic::workspace_service_server::WorkspaceServiceServer;

/// File descriptor sets for gRPC reflection service.
pub const REFLECTION_DESCRIPTORS: &[&[u8]] = &[
    ponix_proto_prost::data_stream::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::document::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::organization::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::gateway::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::user::v1::FILE_DESCRIPTOR_SET,
    ponix_proto_prost::workspace::v1::FILE_DESCRIPTOR_SET,
];

/// All domain services needed to build gRPC routes.
pub struct PonixApiServices {
    pub data_stream_service: Arc<DataStreamService>,
    pub definition_service: Arc<DataStreamDefinitionService>,
    pub document_service: Arc<DocumentService>,
    pub organization_service: Arc<OrganizationService>,
    pub gateway_service: Arc<GatewayService>,
    pub user_service: Arc<UserService>,
    pub workspace_service: Arc<WorkspaceService>,
    pub auth_token_provider: Arc<dyn AuthTokenProvider>,
    pub refresh_token_expiration_days: u64,
    pub secure_cookies: bool,
}

impl PonixApiServices {
    /// Build Routes containing all Ponix API services.
    pub fn into_routes(self, config: &GrpcServerConfig) -> Routes {
        // Create handlers
        let data_stream_handler = DataStreamServiceHandler::new(
            self.data_stream_service,
            self.auth_token_provider.clone(),
        );
        let definition_handler = DataStreamDefinitionServiceHandler::new(
            self.definition_service,
            self.auth_token_provider.clone(),
        );
        let document_handler = DocumentServiceHandler::new(
            self.document_service,
            self.auth_token_provider.clone(),
        );
        let organization_handler = OrganizationServiceHandler::new(
            self.organization_service,
            self.auth_token_provider.clone(),
        );
        let gateway_handler =
            GatewayServiceHandler::new(self.gateway_service, self.auth_token_provider.clone());
        let workspace_handler =
            WorkspaceServiceHandler::new(self.workspace_service, self.auth_token_provider.clone());
        let user_handler = UserServiceHandler::new(
            self.user_service,
            self.auth_token_provider.clone(),
            self.refresh_token_expiration_days,
            self.secure_cookies,
        );

        let max_decode = config.max_decoding_message_size;
        let max_encode = config.max_encoding_message_size;

        // Build routes with all services, applying message size limits
        let mut builder = Routes::builder();
        builder
            .add_service(
                DataStreamServiceServer::new(data_stream_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                DataStreamDefinitionServiceServer::new(definition_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                OrganizationServiceServer::new(organization_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                GatewayServiceServer::new(gateway_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                WorkspaceServiceServer::new(workspace_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                UserServiceServer::new(user_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            )
            .add_service(
                DocumentServiceServer::new(document_handler)
                    .max_decoding_message_size(max_decode)
                    .max_encoding_message_size(max_encode),
            );
        builder.routes()
    }
}

/// Run the Ponix API gRPC server with graceful shutdown.
///
/// This function builds the service routes and delegates to the reusable
/// `run_grpc_server` from common, which handles:
/// - gRPC-Web support (if enabled in config)
/// - CORS configuration
/// - Logging and tracing middleware
/// - Reflection service
pub async fn run_ponix_grpc_server(
    config: GrpcServerConfig,
    services: PonixApiServices,
    cancellation_token: CancellationToken,
) -> Result<(), anyhow::Error> {
    let routes = services.into_routes(&config);
    run_grpc_server(config, routes, REFLECTION_DESCRIPTORS, cancellation_token).await
}
