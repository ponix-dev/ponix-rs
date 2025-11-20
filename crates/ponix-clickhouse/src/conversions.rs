use ponix_domain::types::ProcessedEnvelope as DomainEnvelope;

use crate::models::ProcessedEnvelopeRow;

/// Convert domain ProcessedEnvelope to database ProcessedEnvelopeRow
impl From<&DomainEnvelope> for ProcessedEnvelopeRow {
    fn from(envelope: &DomainEnvelope) -> Self {
        // Convert serde_json::Map to JSON string for ClickHouse storage
        let data_json = serde_json::to_string(&envelope.data)
            .unwrap_or_else(|_| "{}".to_string());

        ProcessedEnvelopeRow {
            organization_id: envelope.organization_id.clone(),
            end_device_id: envelope.end_device_id.clone(),
            occurred_at: envelope.occurred_at,
            processed_at: envelope.processed_at,
            data: data_json,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_to_row_conversion() {
        let mut data_map = serde_json::Map::new();
        data_map.insert("temperature".to_string(), serde_json::json!(23.5));
        data_map.insert("humidity".to_string(), serde_json::json!(65));

        let domain_envelope = DomainEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: data_map,
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.organization_id, "org-123");
        assert_eq!(row.end_device_id, "device-456");
        assert!(row.data.contains("temperature"));
        assert!(row.data.contains("23.5"));
    }

    #[test]
    fn test_empty_data_conversion() {
        let domain_envelope = DomainEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: serde_json::Map::new(),
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.data, "{}");
    }
}
