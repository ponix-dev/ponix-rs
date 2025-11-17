mod config;

use anyhow::Result;
use ponix_clickhouse::{ClickHouseClient, EnvelopeStore, MigrationRunner};
use ponix_nats::{
    create_clickhouse_processor, NatsClient, NatsConsumer, ProcessedEnvelopeProducer,
};
use ponix_proto::envelope::v1::ProcessedEnvelope;
use ponix_runner::Runner;
use prost_types::Timestamp;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!("Starting ponix-all-in-one service");
    debug!("Configuration: {:?}", config);

    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    debug!(
        "Using startup timeout of {:?} for all initialization",
        startup_timeout
    );

    // PHASE 1: Run ClickHouse migrations
    info!("Running ClickHouse migrations...");
    let dsn = format!(
        "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
        config.clickhouse_username,
        config.clickhouse_password,
        config.clickhouse_native_url,
        config.clickhouse_database
    );
    let migration_runner = MigrationRunner::new(
        config.clickhouse_goose_binary_path.clone(),
        config.clickhouse_migrations_dir.clone(),
        "clickhouse".to_string(),
        dsn,
    );

    if let Err(e) = migration_runner.run_migrations().await {
        error!("Failed to run migrations: {}", e);
        std::process::exit(1);
    }

    // PHASE 2: Initialize ClickHouse client
    info!("Connecting to ClickHouse...");
    let clickhouse_client = ClickHouseClient::new(
        &config.clickhouse_url,
        &config.clickhouse_database,
        &config.clickhouse_username,
        &config.clickhouse_password,
    );

    if let Err(e) = clickhouse_client.ping().await {
        error!("Failed to ping ClickHouse: {}", e);
        std::process::exit(1);
    }
    info!("ClickHouse connection established");

    let envelope_store =
        EnvelopeStore::new(clickhouse_client.clone(), "processed_envelopes".to_string());

    // PHASE 3: Connect to NATS
    let nats_client = match NatsClient::connect(&config.nats_url, startup_timeout).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = nats_client.ensure_stream(&config.nats_stream).await {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    // PHASE 4: Create ClickHouse-enabled processor
    let processor = create_clickhouse_processor(move |envelopes| {
        let store = envelope_store.clone();
        async move { store.store_processed_envelopes(envelopes).await }
    });

    // Create consumer using trait-based API
    let consumer_client = nats_client.create_consumer_client();
    let consumer = match NatsConsumer::new(
        consumer_client,
        &config.nats_stream,
        "ponix-all-in-one",
        &config.nats_subject,
        config.nats_batch_size,
        config.nats_batch_wait_secs,
        processor,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create consumer: {}", e);
            std::process::exit(1);
        }
    };

    // Create producer using trait-based API
    let publisher_client = nats_client.create_publisher_client();
    let producer = ProcessedEnvelopeProducer::new(publisher_client, config.nats_stream.clone());

    // Create runner with producer and consumer processes
    let runner = Runner::new()
        .with_app_process({
            let config = config.clone();
            move |ctx| Box::pin(async move { run_producer_service(ctx, config, producer).await })
        })
        .with_app_process(move |ctx| Box::pin(async move { consumer.run(ctx).await }))
        .with_closer(move || {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                nats_client.close().await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service (this will handle the exit)
    runner.run().await;
}

async fn run_producer_service(
    ctx: tokio_util::sync::CancellationToken,
    config: config::ServiceConfig,
    producer: ProcessedEnvelopeProducer,
) -> Result<()> {
    info!("Producer service started successfully");

    let interval = Duration::from_secs(config.interval_secs);

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                info!("Received shutdown signal, stopping producer service");
                break;
            }
            _ = tokio::time::sleep(interval) => {
                // Generate unique ID using xid
                let id = xid::new();

                // Get current time for timestamps
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| Timestamp {
                        seconds: d.as_secs() as i64,
                        nanos: d.subsec_nanos() as i32,
                    })
                    .ok();

                // Create sample data for the envelope
                use prost_types::{value::Kind, Struct, Value};
                use std::collections::BTreeMap;

                let mut fields = BTreeMap::new();
                fields.insert(
                    "temperature".to_string(),
                    Value {
                        kind: Some(Kind::NumberValue(72.5)),
                    },
                );
                fields.insert(
                    "humidity".to_string(),
                    Value {
                        kind: Some(Kind::NumberValue(65.0)),
                    },
                );
                fields.insert(
                    "status".to_string(),
                    Value {
                        kind: Some(Kind::StringValue("active".to_string())),
                    },
                );

                let data = Some(Struct { fields });

                // Create ProcessedEnvelope message
                let envelope = ProcessedEnvelope {
                    end_device_id: id.to_string(),
                    occurred_at: timestamp,
                    data,
                    processed_at: timestamp,
                    organization_id: "example-org".to_string(),
                };

                // Publish to NATS JetStream
                match producer.publish(&envelope).await {
                    Ok(_) => {
                        debug!(
                            end_device_id = %envelope.end_device_id,
                            organization_id = %envelope.organization_id,
                            "{}",
                            config.message
                        );
                    }
                    Err(e) => {
                        error!(
                            end_device_id = %envelope.end_device_id,
                            error = %e,
                            "Failed to publish envelope"
                        );
                    }
                }
            }
        }
    }

    info!("Producer service stopped gracefully");
    Ok(())
}
