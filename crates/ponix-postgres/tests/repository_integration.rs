use ponix_domain::{CreateDeviceInputWithId, DeviceRepository, GetDeviceInput, ListDevicesInput};
use ponix_postgres::{MigrationRunner, PostgresClient, PostgresDeviceRepository};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (ContainerAsync<Postgres>, PostgresDeviceRepository) {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let goose_path = which::which("goose").expect("goose binary not found");

    let migration_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn.clone(),
    );

    migration_runner
        .run_migrations()
        .await
        .expect("Migrations failed");

    // Create client
    let client = PostgresClient::new(&host.to_string(), port, "postgres", "postgres", "postgres", 5)
        .expect("Failed to create client");

    let repository = PostgresDeviceRepository::new(client);

    (postgres, repository)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_and_get_device() {
    let (_container, repo) = setup_test_db().await;

    let input = CreateDeviceInputWithId {
        device_id: "test-device-123".to_string(),
        organization_id: "test-org-456".to_string(),
        name: "Test Device".to_string(),
        payload_conversion: "test conversion".to_string(),
    };

    // Create device
    let created = repo.create_device(input.clone()).await.unwrap();
    assert_eq!(created.device_id, "test-device-123");
    assert_eq!(created.name, "Test Device");
    assert_eq!(created.payload_conversion, "test conversion");
    assert!(created.created_at.is_some());

    // Get device
    let get_input = GetDeviceInput {
        device_id: "test-device-123".to_string(),
    };
    let retrieved = repo.get_device(get_input).await.unwrap();
    assert!(retrieved.is_some());

    let device = retrieved.unwrap();
    assert_eq!(device.device_id, "test-device-123");
    assert_eq!(device.name, "Test Device");
    assert_eq!(device.payload_conversion, "test conversion");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_nonexistent_device() {
    let (_container, repo) = setup_test_db().await;

    let get_input = GetDeviceInput {
        device_id: "nonexistent-device".to_string(),
    };
    let result = repo.get_device(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_by_organization() {
    let (_container, repo) = setup_test_db().await;

    // Create multiple devices
    for i in 1..=3 {
        let input = CreateDeviceInputWithId {
            device_id: format!("device-{}", i),
            organization_id: "test-org".to_string(),
            name: format!("Device {}", i),
            payload_conversion: format!("conversion {}", i),
        };
        repo.create_device(input).await.unwrap();
    }

    // List devices
    let list_input = ListDevicesInput {
        organization_id: "test-org".to_string(),
    };
    let devices = repo.list_devices(list_input).await.unwrap();

    assert_eq!(devices.len(), 3);
    assert!(devices.iter().all(|d| d.organization_id == "test-org"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_for_empty_organization() {
    let (_container, repo) = setup_test_db().await;

    let list_input = ListDevicesInput {
        organization_id: "empty-org".to_string(),
    };
    let devices = repo.list_devices(list_input).await.unwrap();

    assert_eq!(devices.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_device() {
    let (_container, repo) = setup_test_db().await;

    let input = CreateDeviceInputWithId {
        device_id: "duplicate-device".to_string(),
        organization_id: "test-org".to_string(),
        name: "Original Device".to_string(),
        payload_conversion: "test conversion".to_string(),
    };

    // First creation should succeed
    repo.create_device(input.clone()).await.unwrap();

    // Second creation should fail with DeviceAlreadyExists
    let result = repo.create_device(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ponix_domain::DomainError::DeviceAlreadyExists(_)
    ));
}
