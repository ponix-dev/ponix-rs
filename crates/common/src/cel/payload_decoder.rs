use crate::cel::Result;

pub trait PayloadDecoder {
    /// Decode binary payload to JSON value
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
