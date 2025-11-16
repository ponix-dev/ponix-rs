use crate::{ClickHouseClient, ProcessedEnvelopeRow};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost_types::{value::Kind, Timestamp};
use tracing::debug;

#[derive(Clone)]
pub struct EnvelopeStore {
    client: ClickHouseClient,
    table: String,
}

impl EnvelopeStore {
    pub fn new(client: ClickHouseClient, table: String) -> Self {
        Self { client, table }
    }

    pub async fn store_processed_envelopes(&self, envelopes: Vec<ProcessedEnvelope>) -> Result<()> {
        if envelopes.is_empty() {
            return Ok(());
        }

        debug!("Preparing to insert {} envelopes", envelopes.len());

        let mut rows = Vec::with_capacity(envelopes.len());

        for envelope in envelopes {
            // Convert protobuf Struct to JSON string
            let data_json = match envelope.data {
                Some(ref struct_val) => {
                    let fields_map: serde_json::Map<String, serde_json::Value> = struct_val
                        .fields
                        .iter()
                        .filter_map(|(k, v)| {
                            prost_value_to_json(v).map(|json_val| (k.clone(), json_val))
                        })
                        .collect();
                    serde_json::to_string(&fields_map)?
                }
                None => "{}".to_string(),
            };

            let occurred_at = timestamp_to_datetime(
                envelope
                    .occurred_at
                    .as_ref()
                    .context("Missing occurred_at timestamp")?,
            )?;

            let processed_at = timestamp_to_datetime(
                envelope
                    .processed_at
                    .as_ref()
                    .context("Missing processed_at timestamp")?,
            )?;

            rows.push(ProcessedEnvelopeRow {
                organization_id: envelope.organization_id,
                end_device_id: envelope.end_device_id,
                occurred_at,
                processed_at,
                data: data_json,
            });
        }

        // Perform batch insert using clickhouse-rs 0.14 API
        let mut insert = self
            .client
            .get_client()
            .insert::<ProcessedEnvelopeRow>(&self.table)
            .await?;

        for row in &rows {
            insert.write(row).await?;
        }

        insert.end().await?;

        debug!("Successfully inserted {} envelopes", rows.len());
        Ok(())
    }
}

// Helper function to convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: &Timestamp) -> Result<DateTime<Utc>> {
    let seconds = ts.seconds;
    let nanos = ts.nanos as u32;

    DateTime::from_timestamp(seconds, nanos).context("Invalid timestamp")
}

// Helper function to convert protobuf Value to serde_json::Value
fn prost_value_to_json(value: &prost_types::Value) -> Option<serde_json::Value> {
    match &value.kind {
        Some(Kind::NullValue(_)) => Some(serde_json::Value::Null),
        Some(Kind::NumberValue(n)) => Some(serde_json::json!(n)),
        Some(Kind::StringValue(s)) => Some(serde_json::Value::String(s.clone())),
        Some(Kind::BoolValue(b)) => Some(serde_json::Value::Bool(*b)),
        Some(Kind::StructValue(s)) => {
            let map: serde_json::Map<String, serde_json::Value> = s
                .fields
                .iter()
                .filter_map(|(k, v)| prost_value_to_json(v).map(|json_val| (k.clone(), json_val)))
                .collect();
            Some(serde_json::Value::Object(map))
        }
        Some(Kind::ListValue(l)) => {
            let vec: Vec<serde_json::Value> =
                l.values.iter().filter_map(prost_value_to_json).collect();
            Some(serde_json::Value::Array(vec))
        }
        None => None,
    }
}
