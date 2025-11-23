/// Domain entity for a processed envelope
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// Input for storing processed envelopes (batch operation)
#[derive(Debug, Clone)]
pub struct StoreEnvelopesInput {
    pub envelopes: Vec<ProcessedEnvelope>,
}

/// Raw envelope with binary payload before processing
#[derive(Debug, Clone, PartialEq)]
pub struct RawEnvelope {
    pub organization_id: String,
    pub end_device_id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub payload: Vec<u8>,
}
