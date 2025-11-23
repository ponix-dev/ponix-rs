#[cfg(feature = "device")]
pub mod conversions;
#[cfg(feature = "device")]
pub mod device_handler;
#[cfg(feature = "device")]
pub mod error;
#[cfg(feature = "device")]
pub mod server;

pub mod gateway_conversions;
pub mod gateway_handler;
pub mod organization_conversions;
pub mod organization_handler;

#[cfg(feature = "device")]
pub use device_handler::DeviceServiceHandler;
pub use gateway_handler::GatewayServiceHandler;
pub use organization_handler::OrganizationServiceHandler;
#[cfg(feature = "device")]
pub use server::{run_grpc_server, GrpcServerConfig};
