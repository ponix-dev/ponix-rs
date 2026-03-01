use common::domain::{DomainError, DomainResult};

/// Parsed MQTT topic containing organization and data stream IDs
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTopic {
    pub organization_id: String,
    pub data_stream_id: String,
}

/// Parse an MQTT topic in the format `{organization_id}/{data_stream_id}`
///
/// # Arguments
/// * `topic` - The MQTT topic string to parse
///
/// # Returns
/// * `Ok(ParsedTopic)` - Successfully parsed topic
/// * `Err(DomainError)` - Invalid topic format
///
/// # Examples
/// ```
/// use gateway_orchestrator::mqtt::parse_topic;
///
/// let parsed = parse_topic("org-001/device-123").unwrap();
/// assert_eq!(parsed.organization_id, "org-001");
/// assert_eq!(parsed.data_stream_id, "device-123");
/// ```
pub fn parse_topic(topic: &str) -> DomainResult<ParsedTopic> {
    let parts: Vec<&str> = topic.split('/').collect();

    if parts.len() != 2 {
        return Err(DomainError::InvalidGatewayConfig(format!(
            "Invalid topic format '{}': expected '{{org_id}}/{{data_stream_id}}'",
            topic
        )));
    }

    let organization_id = parts[0].trim();
    let data_stream_id = parts[1].trim();

    if organization_id.is_empty() {
        return Err(DomainError::InvalidGatewayConfig(
            "Organization ID cannot be empty in topic".to_string(),
        ));
    }

    if data_stream_id.is_empty() {
        return Err(DomainError::InvalidGatewayConfig(
            "Data stream ID cannot be empty in topic".to_string(),
        ));
    }

    Ok(ParsedTopic {
        organization_id: organization_id.to_string(),
        data_stream_id: data_stream_id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_topic() {
        let result = parse_topic("org-001/device-123");
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.organization_id, "org-001");
        assert_eq!(parsed.data_stream_id, "device-123");
    }

    #[test]
    fn test_parse_topic_with_special_chars() {
        let result = parse_topic("my_org/sensor_temp_01");
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.organization_id, "my_org");
        assert_eq!(parsed.data_stream_id, "sensor_temp_01");
    }

    #[test]
    fn test_parse_topic_missing_device() {
        let result = parse_topic("org-001");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_topic_too_many_segments() {
        let result = parse_topic("org/device/extra");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_topic_empty_org() {
        let result = parse_topic("/device-123");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_topic_empty_device() {
        let result = parse_topic("org-001/");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_topic_empty_string() {
        let result = parse_topic("");
        assert!(result.is_err());
    }
}
