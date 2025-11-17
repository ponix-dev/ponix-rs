use chrono::Utc;
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore, MigrationRunner};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use prost_types::Timestamp;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::Image;
use testcontainers_modules::clickhouse::ClickHouse;

/// Custom ClickHouse image with version 24.10 for JSON type support
/// Exposes both HTTP (8123) and Native TCP (9000) ports
#[derive(Debug, Clone)]
struct ClickHouse24 {
    inner: ClickHouse,
    ports: Vec<ContainerPort>,
}

impl Default for ClickHouse24 {
    fn default() -> Self {
        Self {
            inner: ClickHouse::default(),
            ports: vec![
                ContainerPort::Tcp(8123), // HTTP interface
                ContainerPort::Tcp(9000), // Native TCP interface (for goose migrations)
            ],
        }
    }
}

impl Image for ClickHouse24 {
    fn name(&self) -> &str {
        "clickhouse/clickhouse-server"
    }

    fn tag(&self) -> &str {
        "24.10"
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        // Use default ready conditions from testcontainers-modules ClickHouse
        // The HTTP wait strategy polls every 100ms until ready
        self.inner.ready_conditions()
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<
        Item = (
            impl Into<std::borrow::Cow<'_, str>>,
            impl Into<std::borrow::Cow<'_, str>>,
        ),
    > {
        self.inner.env_vars()
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &self.ports
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_clickhouse_connection() {
    // Start ClickHouse 24.10 container
    let clickhouse = ClickHouse24::default().start().await.unwrap();

    let host = clickhouse.get_host().await.unwrap();
    let port = clickhouse.get_host_port_ipv4(8123).await.unwrap();

    // Create client with http:// URL format
    let client = ClickHouseClient::new(
        &format!("http://{}:{}", host, port),
        "default",
        "default",
        "",
    );

    // Test connection
    client
        .ping()
        .await
        .expect("Should be able to ping ClickHouse");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_migrations_and_batch_write() {
    // Start ClickHouse 24.10 container
    let clickhouse = ClickHouse24::default().start().await.unwrap();

    let host = clickhouse.get_host().await.unwrap();
    let http_port = clickhouse.get_host_port_ipv4(8123).await.unwrap();
    let native_port = clickhouse.get_host_port_ipv4(9000).await.unwrap();

    let url_with_scheme = format!("http://{}:{}", host, http_port);
    let native_url = format!("{}:{}", host, native_port); // For migrations DSN (native protocol)

    // PHASE 1: Run migrations
    // Use CARGO_MANIFEST_DIR to get absolute path to migrations directory
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        native_url,
        "default".to_string(),
        "default".to_string(),
        "".to_string(),
    );

    migration_runner
        .run_migrations()
        .await
        .expect("Migrations should run successfully");

    // PHASE 2: Create client and store
    let client = ClickHouseClient::new(&url_with_scheme, "default", "default", "");

    client
        .ping()
        .await
        .expect("Should be able to ping ClickHouse after migrations");

    let store = EnvelopeStore::new(client.clone(), "processed_envelopes".to_string());

    // PHASE 3: Create test envelopes
    let now_timestamp = {
        let now = Utc::now();
        Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }
    };

    let envelopes = vec![
        ProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-001".to_string(),
            occurred_at: Some(now_timestamp.clone()),
            processed_at: Some(now_timestamp.clone()),
            data: Some(prost_types::Struct {
                fields: [
                    (
                        "temperature".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::NumberValue(25.5)),
                        },
                    ),
                    (
                        "humidity".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::NumberValue(60.0)),
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }),
        },
        ProcessedEnvelope {
            organization_id: "org-456".to_string(),
            end_device_id: "device-002".to_string(),
            occurred_at: Some(now_timestamp.clone()),
            processed_at: Some(now_timestamp.clone()),
            data: Some(prost_types::Struct {
                fields: [
                    (
                        "status".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::StringValue("online".to_string())),
                        },
                    ),
                    (
                        "battery".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::NumberValue(87.5)),
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }),
        },
    ];

    // PHASE 4: Write envelopes to ClickHouse
    store
        .store_processed_envelopes(envelopes)
        .await
        .expect("Should be able to write envelopes");

    // PHASE 5: Verify data was written by querying count
    let count: u64 = client
        .get_client()
        .query("SELECT count() FROM processed_envelopes")
        .fetch_one()
        .await
        .expect("Should be able to query count");

    assert_eq!(count, 2, "Should have 2 envelopes in the database");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_large_batch_write() {
    // Start ClickHouse 24.10 container
    let clickhouse = ClickHouse24::default().start().await.unwrap();

    let host = clickhouse.get_host().await.unwrap();
    let http_port = clickhouse.get_host_port_ipv4(8123).await.unwrap();
    let native_port = clickhouse.get_host_port_ipv4(9000).await.unwrap();

    let url_with_scheme = format!("http://{}:{}", host, http_port);
    let native_url = format!("{}:{}", host, native_port);

    // Run migrations first
    // Use CARGO_MANIFEST_DIR to get absolute path to migrations directory
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        native_url,
        "default".to_string(),
        "default".to_string(),
        "".to_string(),
    );

    migration_runner
        .run_migrations()
        .await
        .expect("Migrations should run successfully");

    // Create client and store
    let client = ClickHouseClient::new(&url_with_scheme, "default", "default", "");
    let store = EnvelopeStore::new(client.clone(), "processed_envelopes".to_string());

    // Create a large batch of test envelopes (1000 envelopes)
    let now_timestamp = {
        let now = Utc::now();
        Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }
    };

    let mut envelopes = Vec::with_capacity(1000);
    for i in 0..1000 {
        envelopes.push(ProcessedEnvelope {
            organization_id: format!("org-{}", i % 10), // 10 different orgs
            end_device_id: format!("device-{:04}", i),
            occurred_at: Some(now_timestamp.clone()),
            processed_at: Some(now_timestamp.clone()),
            data: Some(prost_types::Struct {
                fields: [
                    (
                        "index".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::NumberValue(i as f64)),
                        },
                    ),
                    (
                        "batch_test".to_string(),
                        prost_types::Value {
                            kind: Some(prost_types::value::Kind::BoolValue(true)),
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }),
        });
    }

    // Write large batch
    store
        .store_processed_envelopes(envelopes)
        .await
        .expect("Should be able to write 1000 envelopes");

    // Verify count
    let count: u64 = client
        .get_client()
        .query("SELECT count() FROM processed_envelopes")
        .fetch_one()
        .await
        .expect("Should be able to query count");

    assert_eq!(count, 1000, "Should have 1000 envelopes in the database");

    // Verify we can query by organization_id
    let org_count: u64 = client
        .get_client()
        .query("SELECT count() FROM processed_envelopes WHERE organization_id = ?")
        .bind("org-0")
        .fetch_one()
        .await
        .expect("Should be able to query by organization");

    assert_eq!(
        org_count, 100,
        "Should have 100 envelopes for org-0 (every 10th envelope)"
    );
}
