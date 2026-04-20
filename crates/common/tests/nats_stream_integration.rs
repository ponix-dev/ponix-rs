#![cfg(feature = "integration-tests")]

use async_nats::jetstream::stream::Config as StreamConfig;
use common::nats::NatsClient;
use std::sync::Arc;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

async fn setup_nats() -> (ContainerAsync<GenericImage>, Arc<NatsClient>) {
    let nats_container = GenericImage::new("nats", "latest")
        .with_exposed_port(4222.into())
        .with_cmd(["-js"])
        .start()
        .await
        .unwrap();

    let nats_host = nats_container.get_host().await.unwrap();
    let nats_port = nats_container.get_host_port_ipv4(4222).await.unwrap();
    let nats_url = format!("nats://{}:{}", nats_host, nats_port);

    let nats_client = Arc::new(
        NatsClient::connect(&nats_url, Duration::from_secs(10))
            .await
            .expect("Failed to connect to NATS"),
    );

    (nats_container, nats_client)
}

#[tokio::test]
async fn test_ensure_stream_creates_with_custom_config() {
    let (_container, client) = setup_nats().await;

    let stream_name = "custom_stream";
    let max_age = Duration::from_secs(604800); // 7 days

    client
        .ensure_stream(StreamConfig {
            name: stream_name.to_string(),
            subjects: vec![format!("{}.*", stream_name)],
            description: Some("Custom stream for testing ensure_stream with config".to_string()),
            max_age,
            ..Default::default()
        })
        .await
        .expect("Failed to create stream");

    // Verify stream exists with correct config
    let mut stream = client
        .jetstream()
        .get_stream(stream_name)
        .await
        .expect("Stream should exist");
    let info = stream.info().await.expect("Failed to get stream info");

    assert_eq!(info.config.name, stream_name);
    assert_eq!(info.config.subjects, vec!["custom_stream.*"]);
    assert_eq!(info.config.max_age, max_age);
    assert_eq!(
        info.config.description,
        Some("Custom stream for testing ensure_stream with config".to_string())
    );
}

#[tokio::test]
async fn test_ensure_stream_is_idempotent() {
    let (_container, client) = setup_nats().await;

    let config = StreamConfig {
        name: "idempotent_test".to_string(),
        subjects: vec!["idempotent_test.*".to_string()],
        ..Default::default()
    };

    // Create twice — second call should succeed without error
    client
        .ensure_stream(config.clone())
        .await
        .expect("First create failed");
    client
        .ensure_stream(config)
        .await
        .expect("Second create (idempotent) failed");

    // Stream should still exist
    client
        .jetstream()
        .get_stream("idempotent_test")
        .await
        .expect("Stream should still exist");
}

#[tokio::test]
async fn test_ensure_stream_captures_messages() {
    let (_container, client) = setup_nats().await;

    let stream_name = "capture_test";

    client
        .ensure_stream(StreamConfig {
            name: stream_name.to_string(),
            subjects: vec![format!("{}.*", stream_name)],
            ..Default::default()
        })
        .await
        .expect("Failed to create stream");

    // Publish a message to a subject matching the stream pattern
    let subject = format!("{}.test-doc-id", stream_name);
    let payload = bytes::Bytes::from("test update payload");

    client
        .jetstream()
        .publish(subject, payload)
        .await
        .expect("Failed to publish")
        .await
        .expect("Failed to get ack");

    // Verify message is in the stream
    let mut stream = client
        .jetstream()
        .get_stream(stream_name)
        .await
        .expect("Stream should exist");
    let info = stream.info().await.expect("Failed to get stream info");

    assert_eq!(info.state.messages, 1, "Stream should contain 1 message");
}
