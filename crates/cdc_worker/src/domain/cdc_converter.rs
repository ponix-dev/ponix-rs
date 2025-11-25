use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;

/// Trait for converting CDC operations to NATS messages
#[async_trait]
pub trait CdcConverter: Send + Sync {
    /// Convert INSERT operation to bytes
    async fn convert_insert(&self, data: Value) -> anyhow::Result<Bytes>;

    /// Convert UPDATE operation to bytes
    async fn convert_update(&self, old: Value, new: Value) -> anyhow::Result<Bytes>;

    /// Convert DELETE operation to bytes
    async fn convert_delete(&self, data: Value) -> anyhow::Result<Bytes>;
}
