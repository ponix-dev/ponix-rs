pub mod cayenne_lpp;
pub mod cel;
pub mod cel_converter;
mod error;

pub use cayenne_lpp::CayenneLppDecoder;
pub use cel::CelEnvironment;
pub use cel_converter::CelPayloadConverter;
pub use error::{PayloadError, Result};

/// Trait for decoding binary payload formats to JSON
pub trait PayloadDecoder {
    /// Decode binary payload to JSON value
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
