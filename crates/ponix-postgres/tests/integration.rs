use ponix_postgres::{Device, DeviceStore, MigrationRunner, PostgresClient};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_postgres_connection() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    client.ping().await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_device_registration_and_query() {
    // Start PostgreSQL container
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    // Create client
    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    client.ping().await.unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    // Create device store
    let device_store = DeviceStore::new(client.clone());

    // Test 1: Register a device
    let device = Device {
        device_id: "device-001".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device 1".to_string(),
    };

    device_store.register_device(&device).await.unwrap();

    // Test 2: Query device by ID
    let retrieved = device_store.get_device_by_id("device-001").await.unwrap();

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.device_id, "device-001");
    assert_eq!(retrieved.organization_id, "org-001");
    assert_eq!(retrieved.device_name, "Test Device 1");

    // Test 3: Query non-existent device
    let not_found = device_store.get_device_by_id("device-999").await.unwrap();
    assert!(not_found.is_none());

    // Test 4: Register another device in the same organization
    let device2 = Device {
        device_id: "device-002".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device 2".to_string(),
    };

    device_store.register_device(&device2).await.unwrap();

    // Test 5: List devices by organization
    let devices = device_store
        .list_devices_by_organization("org-001")
        .await
        .unwrap();

    assert_eq!(devices.len(), 2);
    assert!(devices.iter().any(|d| d.device_id == "device-001"));
    assert!(devices.iter().any(|d| d.device_id == "device-002"));

    // Test 6: Update device name
    device_store
        .update_device_name("device-001", "Updated Device Name")
        .await
        .unwrap();

    let updated = device_store
        .get_device_by_id("device-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.device_name, "Updated Device Name");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_for_empty_organization() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    let device_store = DeviceStore::new(client);

    let devices = device_store
        .list_devices_by_organization("org-999")
        .await
        .unwrap();

    assert_eq!(devices.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_duplicate_device_id_fails() {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let migration_runner = MigrationRunner::new(
        "goose".to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn,
    );

    migration_runner.run_migrations().await.unwrap();

    let device_store = DeviceStore::new(client);

    let device = Device {
        device_id: "device-duplicate".to_string(),
        organization_id: "org-001".to_string(),
        device_name: "Test Device".to_string(),
    };

    // First registration should succeed
    device_store.register_device(&device).await.unwrap();

    // Second registration with same ID should fail
    let result = device_store.register_device(&device).await;
    assert!(result.is_err());
}
