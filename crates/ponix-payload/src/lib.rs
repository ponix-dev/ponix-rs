mod error;
pub mod cayenne_lpp;

pub use error::{PayloadError, Result};
pub use cayenne_lpp::CayenneLppDecoder;

/// Trait for decoding binary payload formats to JSON
pub trait PayloadDecoder {
    /// Decode binary payload to JSON value
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
