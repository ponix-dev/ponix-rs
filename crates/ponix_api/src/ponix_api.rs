use crate::domain::{
    DeviceService, EndDeviceDefinitionService, GatewayService, OrganizationService, UserService,
};
use crate::grpc::run_ponix_grpc_server;
use common::auth::AuthTokenProvider;
use common::grpc::GrpcServerConfig;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct PonixApi {
    device_service: Arc<DeviceService>,
    definition_service: Arc<EndDeviceDefinitionService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    user_service: Arc<UserService>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    config: GrpcServerConfig,
    refresh_token_expiration_days: u64,
    secure_cookies: bool,
}

impl PonixApi {
    pub fn new(
        device_service: Arc<DeviceService>,
        definition_service: Arc<EndDeviceDefinitionService>,
        organization_service: Arc<OrganizationService>,
        gateway_service: Arc<GatewayService>,
        user_service: Arc<UserService>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
        config: GrpcServerConfig,
        refresh_token_expiration_days: u64,
        secure_cookies: bool,
    ) -> Self {
        debug!("Initializing Ponix API module");
        Self {
            device_service,
            definition_service,
            organization_service,
            gateway_service,
            user_service,
            auth_token_provider,
            config,
            refresh_token_expiration_days,
            secure_cookies,
        }
    }

    pub fn into_runner_process(
        self,
    ) -> impl FnOnce(
        CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
    > {
        move |ctx| {
            Box::pin(async move {
                run_ponix_grpc_server(
                    self.config,
                    self.device_service,
                    self.definition_service,
                    self.organization_service,
                    self.gateway_service,
                    self.user_service,
                    self.auth_token_provider,
                    self.refresh_token_expiration_days,
                    self.secure_cookies,
                    ctx,
                )
                .await
            })
        }
    }
}
