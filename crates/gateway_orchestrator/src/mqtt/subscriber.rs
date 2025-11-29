use crate::domain::GatewayOrchestrationServiceConfig;
use crate::mqtt::parse_topic;
use common::domain::{DomainError, DomainResult, Gateway, GatewayConfig, RawEnvelope, RawEnvelopeProducer};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, info_span, instrument, warn, Instrument, Span};

/// Run the MQTT subscriber process for a gateway
///
/// Subscribes to `{org_id}/+` on the configured MQTT broker and publishes
/// received messages as RawEnvelopes to NATS.
#[instrument(
    name = "mqtt_subscriber",
    skip_all,
    fields(
        gateway_id = %gateway.gateway_id,
        organization_id = %gateway.organization_id,
    )
)]
pub async fn run_mqtt_subscriber(
    gateway: Gateway,
    config: GatewayOrchestrationServiceConfig,
    process_token: CancellationToken,
    shutdown_token: CancellationToken,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
) {
    let broker_url = match &gateway.gateway_config {
        GatewayConfig::Emqx(emqx) => &emqx.broker_url,
    };

    info!(
        gateway_id = %gateway.gateway_id,
        organization_id = %gateway.organization_id,
        broker_url = %broker_url,
        "starting MQTT subscriber"
    );

    let mut retry_count = 0;

    loop {
        // Check for cancellation before attempting connection
        if process_token.is_cancelled() || shutdown_token.is_cancelled() {
            debug!(gateway_id = %gateway.gateway_id, "MQTT subscriber cancelled before connection");
            break;
        }

        match run_mqtt_connection(
            &gateway,
            broker_url,
            &process_token,
            &shutdown_token,
            Arc::clone(&raw_envelope_producer),
        )
        .await
        {
            Ok(()) => {
                // Clean exit (cancellation)
                debug!(gateway_id = %gateway.gateway_id, "MQTT subscriber stopped cleanly");
                break;
            }
            Err(e) => {
                error!(
                    gateway_id = %gateway.gateway_id,
                    error = %e,
                    "MQTT connection error"
                );

                retry_count += 1;
                if retry_count >= config.max_retry_attempts {
                    error!(
                        gateway_id = %gateway.gateway_id,
                        max_retries = config.max_retry_attempts,
                        "max retry attempts reached, stopping MQTT subscriber"
                    );
                    break;
                }

                warn!(
                    gateway_id = %gateway.gateway_id,
                    attempt = retry_count,
                    max_attempts = config.max_retry_attempts,
                    "retrying MQTT connection"
                );

                // Wait before retry with cancellation check
                tokio::select! {
                    _ = process_token.cancelled() => break,
                    _ = shutdown_token.cancelled() => break,
                    _ = tokio::time::sleep(config.retry_delay()) => {}
                }
            }
        }
    }

    info!(gateway_id = %gateway.gateway_id, "MQTT subscriber stopped");
}

/// Run a single MQTT connection session
#[instrument(
    name = "mqtt_connection",
    skip_all,
    fields(
        gateway_id = %gateway.gateway_id,
        broker_url = %broker_url,
    )
)]
async fn run_mqtt_connection(
    gateway: &Gateway,
    broker_url: &str,
    process_token: &CancellationToken,
    shutdown_token: &CancellationToken,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
) -> DomainResult<()> {
    // Parse broker URL to extract host and port
    let (host, port) = parse_broker_url(broker_url)?;

    // Create MQTT client
    let client_id = format!("ponix-{}", gateway.gateway_id);
    let mut mqtt_options = MqttOptions::new(&client_id, host, port);
    mqtt_options.set_keep_alive(Duration::from_secs(30));
    mqtt_options.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

    // Subscribe to org_id/+ pattern (single-level wildcard for device_id)
    let subscribe_topic = format!("{}/+", gateway.organization_id);
    client
        .subscribe(&subscribe_topic, QoS::AtLeastOnce)
        .await
        .map_err(|e| {
            DomainError::RepositoryError(anyhow::anyhow!("Failed to subscribe: {}", e))
        })?;

    info!(
        gateway_id = %gateway.gateway_id,
        topic = %subscribe_topic,
        "subscribed to MQTT topic"
    );

    // Event loop
    loop {
        tokio::select! {
            _ = process_token.cancelled() => {
                debug!(gateway_id = %gateway.gateway_id, "process cancellation received");
                let _ = client.disconnect().await;
                return Ok(());
            }
            _ = shutdown_token.cancelled() => {
                debug!(gateway_id = %gateway.gateway_id, "shutdown signal received");
                let _ = client.disconnect().await;
                return Ok(());
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        handle_mqtt_message(
                            gateway,
                            &publish.topic,
                            &publish.payload,
                            Arc::clone(&raw_envelope_producer),
                        )
                        .await;
                    }
                    Ok(Event::Incoming(Packet::SubAck(_))) => {
                        debug!(gateway_id = %gateway.gateway_id, "subscription acknowledged");
                    }
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        info!(gateway_id = %gateway.gateway_id, "connected to MQTT broker");
                    }
                    Ok(Event::Incoming(Packet::PingResp)) => {
                        // Ping response - connection is healthy
                    }
                    Ok(_) => {
                        // Other events (outgoing, etc.)
                    }
                    Err(e) => {
                        return Err(DomainError::RepositoryError(
                            anyhow::anyhow!("MQTT event loop error: {}", e),
                        ));
                    }
                }
            }
        }
    }
}

/// Handle an incoming MQTT message
///
/// Creates a new independent trace for each message (not nested under the gateway process trace).
pub(crate) async fn handle_mqtt_message(
    gateway: &Gateway,
    topic: &str,
    payload: &[u8],
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
) {
    // Create a new root span for this message (independent trace)
    let span = info_span!(
        parent: Span::none(),
        "mqtt_message",
        gateway_id = %gateway.gateway_id,
        organization_id = %gateway.organization_id,
        topic = %topic,
        payload_size = payload.len(),
        device_id = tracing::field::Empty,
    );

    async {
        // Parse topic to extract org_id and device_id
        let parsed = match parse_topic(topic) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    error = %e,
                    "failed to parse MQTT topic, skipping message"
                );
                return;
            }
        };

        // Record device_id in the current span
        Span::current().record("device_id", &parsed.end_device_id);

        // Verify organization matches gateway config
        if parsed.organization_id != gateway.organization_id {
            warn!(
                topic_org = %parsed.organization_id,
                gateway_org = %gateway.organization_id,
                "organization ID mismatch, skipping message"
            );
            return;
        }

        // Create RawEnvelope
        let envelope = RawEnvelope {
            organization_id: parsed.organization_id,
            end_device_id: parsed.end_device_id.clone(),
            occurred_at: chrono::Utc::now(),
            payload: payload.to_vec(),
        };

        // Publish to NATS (fire-and-forget with error logging)
        if let Err(e) = raw_envelope_producer.publish_raw_envelope(&envelope).await {
            error!(error = %e, "failed to publish RawEnvelope to NATS");
        } else {
            debug!("published RawEnvelope to NATS");
        }
    }
    .instrument(span)
    .await
}

/// Parse broker URL in format mqtt://host:port or tcp://host:port or host:port
fn parse_broker_url(url: &str) -> DomainResult<(&str, u16)> {
    let url = url.trim_start_matches("mqtt://");
    let url = url.trim_start_matches("tcp://");

    let parts: Vec<&str> = url.split(':').collect();
    match parts.len() {
        1 => Ok((parts[0], 1883)), // Default MQTT port
        2 => {
            let port = parts[1].parse::<u16>().map_err(|_| {
                DomainError::InvalidGatewayConfig(format!("Invalid port in broker URL: {}", parts[1]))
            })?;
            Ok((parts[0], port))
        }
        _ => Err(DomainError::InvalidGatewayConfig(format!(
            "Invalid broker URL format: {}",
            url
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{EmqxGatewayConfig, MockRawEnvelopeProducer};

    fn create_test_gateway(gateway_id: &str, org_id: &str) -> Gateway {
        Gateway {
            gateway_id: gateway_id.to_string(),
            organization_id: org_id.to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
            }),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            deleted_at: None,
        }
    }

    #[test]
    fn test_parse_broker_url_with_port() {
        let (host, port) = parse_broker_url("mqtt://localhost:1883").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_broker_url_without_scheme() {
        let (host, port) = parse_broker_url("emqx.example.com:8883").unwrap();
        assert_eq!(host, "emqx.example.com");
        assert_eq!(port, 8883);
    }

    #[test]
    fn test_parse_broker_url_default_port() {
        let (host, port) = parse_broker_url("mqtt://broker.local").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_broker_url_tcp_scheme() {
        let (host, port) = parse_broker_url("tcp://mqtt.example.com:1883").unwrap();
        assert_eq!(host, "mqtt.example.com");
        assert_eq!(port, 1883);
    }

    #[tokio::test]
    async fn test_handle_mqtt_message_success() {
        let gateway = create_test_gateway("gw-001", "org-001");

        let mut mock_producer = MockRawEnvelopeProducer::new();
        mock_producer
            .expect_publish_raw_envelope()
            .withf(|envelope: &RawEnvelope| {
                envelope.organization_id == "org-001"
                    && envelope.end_device_id == "device-123"
                    && envelope.payload == vec![0x01, 0x02, 0x03]
            })
            .times(1)
            .returning(|_| Ok(()));

        let producer: Arc<dyn RawEnvelopeProducer> = Arc::new(mock_producer);

        handle_mqtt_message(&gateway, "org-001/device-123", &[0x01, 0x02, 0x03], producer).await;
    }

    #[tokio::test]
    async fn test_handle_mqtt_message_org_mismatch() {
        let gateway = create_test_gateway("gw-001", "org-001");

        let mut mock_producer = MockRawEnvelopeProducer::new();
        // Should NOT be called due to org mismatch
        mock_producer.expect_publish_raw_envelope().times(0);

        let producer: Arc<dyn RawEnvelopeProducer> = Arc::new(mock_producer);

        handle_mqtt_message(
            &gateway,
            "different-org/device-123",
            &[0x01, 0x02, 0x03],
            producer,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_mqtt_message_invalid_topic() {
        let gateway = create_test_gateway("gw-001", "org-001");

        let mut mock_producer = MockRawEnvelopeProducer::new();
        // Should NOT be called due to invalid topic
        mock_producer.expect_publish_raw_envelope().times(0);

        let producer: Arc<dyn RawEnvelopeProducer> = Arc::new(mock_producer);

        handle_mqtt_message(&gateway, "invalid-topic-format", &[0x01, 0x02, 0x03], producer).await;
    }
}
