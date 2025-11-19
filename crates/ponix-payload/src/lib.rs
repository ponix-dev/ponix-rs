pub mod cayenne_lpp;
pub mod cel;
mod error;

pub use cayenne_lpp::CayenneLppDecoder;
pub use cel::CelEnvironment;
pub use error::{PayloadError, Result};

/// Trait for decoding binary payload formats to JSON
pub trait PayloadDecoder {
    /// Decode binary payload to JSON value
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
