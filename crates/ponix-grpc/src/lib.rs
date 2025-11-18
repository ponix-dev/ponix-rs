#[cfg(feature = "device")]
pub mod conversions;
#[cfg(feature = "device")]
pub mod device_handler;
#[cfg(feature = "device")]
pub mod error;
#[cfg(feature = "device")]
pub mod server;

#[cfg(feature = "device")]
pub use device_handler::DeviceServiceHandler;
#[cfg(feature = "device")]
pub use server::{run_grpc_server, GrpcServerConfig};
