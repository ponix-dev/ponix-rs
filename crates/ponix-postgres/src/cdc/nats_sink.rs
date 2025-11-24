use crate::cdc::traits::EntityConfig;
use etl::destination::Destination;
use etl::error::{ErrorKind, EtlResult};
use etl::etl_error;
use etl::types::{DeleteEvent, Event, InsertEvent, TableId, TableRow, UpdateEvent};
use etl_postgres::types::TableSchema;
use ponix_nats::JetStreamPublisher;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

/// NATS sink for ETL replication events.
///
/// `NatsSink` implements the ETL `Destination` trait and publishes
/// CDC events to NATS JetStream with configurable entity-based routing.
pub struct NatsSink {
    publisher: Arc<dyn JetStreamPublisher>,
    configs: Arc<HashMap<TableId, EntityConfig>>,
    table_schemas: Arc<HashMap<TableId, TableSchema>>,
}

impl Clone for NatsSink {
    fn clone(&self) -> Self {
        Self {
            publisher: Arc::clone(&self.publisher),
            configs: Arc::clone(&self.configs),
            table_schemas: Arc::clone(&self.table_schemas),
        }
    }
}

impl NatsSink {
    /// Creates a new NATS sink with the given publisher and entity configurations.
    ///
    /// The sink will route events based on table names matched to entity configs.
    pub fn new(publisher: Arc<dyn JetStreamPublisher>, _configs: Vec<EntityConfig>) -> Self {
        // We'll populate table IDs when we receive Relation events
        // TODO: Store configs and map them to table IDs when Relation events are received
        Self {
            publisher,
            configs: Arc::new(HashMap::new()),
            table_schemas: Arc::new(HashMap::new()),
        }
    }

    /// Gets the subject name for a given entity and operation.
    fn get_subject(&self, entity: &str, operation: &str) -> String {
        format!("{}.{}", entity, operation)
    }

    /// Converts a TableRow to a JSON Value using the table schema.
    fn table_row_to_json(&self, table_id: &TableId, table_row: &TableRow) -> EtlResult<Value> {
        let schema = self
            .table_schemas
            .get(table_id)
            .ok_or_else(|| etl_error!(ErrorKind::MissingTableSchema, "Table schema not found"))?;

        let mut obj = serde_json::Map::new();

        for (i, cell) in table_row.values.iter().enumerate() {
            if let Some(column) = schema.column_schemas.get(i) {
                let value = cell_to_json_value(cell);
                obj.insert(column.name.clone(), value);
            }
        }

        Ok(Value::Object(obj))
    }

    /// Handles an insert event by converting and publishing to NATS.
    async fn handle_insert(&self, event: InsertEvent) -> EtlResult<()> {
        let config = match self.configs.get(&event.table_id) {
            Some(c) => c,
            None => {
                warn!(
                    "No CDC config for table {:?}, skipping insert event",
                    event.table_id
                );
                return Ok(());
            }
        };

        let data = self.table_row_to_json(&event.table_id, &event.table_row)?;

        match config.converter.convert_insert(data).await {
            Ok(payload) => {
                let subject = self.get_subject(&config.entity_name, "create");
                if let Err(e) = self.publisher.publish(subject.clone(), payload).await {
                    error!("Failed to publish insert event to {}: {}", subject, e);
                    return Err(etl_error!(
                        ErrorKind::DestinationIoError,
                        "Failed to publish insert event",
                        format!("Failed to publish to {}: {}", subject, e)
                    ));
                }
                info!("Published insert event to {}", subject);
                Ok(())
            }
            Err(e) => {
                error!("Failed to convert insert event: {}", e);
                Err(etl_error!(
                    ErrorKind::ConversionError,
                    "Failed to convert insert event",
                    format!("Conversion error: {}", e)
                ))
            }
        }
    }

    /// Handles an update event by converting and publishing to NATS.
    async fn handle_update(&self, event: UpdateEvent) -> EtlResult<()> {
        let config = match self.configs.get(&event.table_id) {
            Some(c) => c,
            None => {
                warn!(
                    "No CDC config for table {:?}, skipping update event",
                    event.table_id
                );
                return Ok(());
            }
        };

        let new_data = self.table_row_to_json(&event.table_id, &event.table_row)?;
        let old_data = if let Some((_, ref old_row)) = event.old_table_row {
            self.table_row_to_json(&event.table_id, old_row)?
        } else {
            json!({})
        };

        match config.converter.convert_update(old_data, new_data).await {
            Ok(payload) => {
                let subject = self.get_subject(&config.entity_name, "update");
                if let Err(e) = self.publisher.publish(subject.clone(), payload).await {
                    error!("Failed to publish update event to {}: {}", subject, e);
                    return Err(etl_error!(
                        ErrorKind::DestinationIoError,
                        "Failed to publish update event",
                        format!("Failed to publish to {}: {}", subject, e)
                    ));
                }
                info!("Published update event to {}", subject);
                Ok(())
            }
            Err(e) => {
                error!("Failed to convert update event: {}", e);
                Err(etl_error!(
                    ErrorKind::ConversionError,
                    "Failed to convert update event",
                    format!("Conversion error: {}", e)
                ))
            }
        }
    }

    /// Handles a delete event by converting and publishing to NATS.
    async fn handle_delete(&self, event: DeleteEvent) -> EtlResult<()> {
        let config = match self.configs.get(&event.table_id) {
            Some(c) => c,
            None => {
                warn!(
                    "No CDC config for table {:?}, skipping delete event",
                    event.table_id
                );
                return Ok(());
            }
        };

        let data = if let Some((_, ref old_row)) = event.old_table_row {
            self.table_row_to_json(&event.table_id, old_row)?
        } else {
            json!({})
        };

        match config.converter.convert_delete(data).await {
            Ok(payload) => {
                let subject = self.get_subject(&config.entity_name, "delete");
                if let Err(e) = self.publisher.publish(subject.clone(), payload).await {
                    error!("Failed to publish delete event to {}: {}", subject, e);
                    return Err(etl_error!(
                        ErrorKind::DestinationIoError,
                        "Failed to publish delete event",
                        format!("Failed to publish to {}: {}", subject, e)
                    ));
                }
                info!("Published delete event to {}", subject);
                Ok(())
            }
            Err(e) => {
                error!("Failed to convert delete event: {}", e);
                Err(etl_error!(
                    ErrorKind::ConversionError,
                    "Failed to convert delete event",
                    format!("Conversion error: {}", e)
                ))
            }
        }
    }
}

impl Destination for NatsSink {
    fn name() -> &'static str {
        "nats"
    }

    async fn truncate_table(&self, table_id: TableId) -> EtlResult<()> {
        info!(
            "Truncate table requested for {:?} (no-op for NATS sink)",
            table_id
        );
        // For NATS, we don't need to do anything on truncate
        Ok(())
    }

    async fn write_table_rows(
        &self,
        table_id: TableId,
        table_rows: Vec<TableRow>,
    ) -> EtlResult<()> {
        info!(
            "Write table rows for {:?}: {} rows (skipping initial sync for NATS sink)",
            table_id,
            table_rows.len()
        );
        // We skip initial table synchronization for NATS - we only care about real-time changes
        Ok(())
    }

    async fn write_events(&self, events: Vec<Event>) -> EtlResult<()> {
        info!("Processing batch of {} events", events.len());

        for event in events {
            match event {
                Event::Relation(rel_event) => {
                    // This is a mutable operation, but we need to make NatsSink mutable
                    // For now, we'll track schemas differently
                    info!(
                        "Received relation event for table: {} (ID: {:?})",
                        rel_event.table_schema.name, rel_event.table_schema.id
                    );
                }
                Event::Insert(insert_event) => {
                    if let Err(e) = self.handle_insert(insert_event).await {
                        error!("Failed to handle insert event: {:?}", e);
                        // Continue processing other events
                    }
                }
                Event::Update(update_event) => {
                    if let Err(e) = self.handle_update(update_event).await {
                        error!("Failed to handle update event: {:?}", e);
                        // Continue processing other events
                    }
                }
                Event::Delete(delete_event) => {
                    if let Err(e) = self.handle_delete(delete_event).await {
                        error!("Failed to handle delete event: {:?}", e);
                        // Continue processing other events
                    }
                }
                Event::Begin(_) => {
                    // Transaction begin - we don't need to do anything
                }
                Event::Commit(_) => {
                    // Transaction commit - we don't need to do anything
                }
                Event::Truncate(truncate_event) => {
                    info!(
                        "Received truncate event for tables: {:?}",
                        truncate_event.rel_ids
                    );
                    // For NATS, we don't publish truncate events
                }
                Event::Unsupported => {
                    warn!("Received unsupported event type");
                }
            }
        }

        Ok(())
    }
}

/// Converts an ETL Cell to a JSON value.
fn cell_to_json_value(cell: &etl::types::Cell) -> Value {
    use etl::types::Cell;

    match cell {
        Cell::Null => Value::Null,
        Cell::Bool(b) => json!(b),
        Cell::String(s) => json!(s),
        Cell::I16(i) => json!(i),
        Cell::I32(i) => json!(i),
        Cell::U32(u) => json!(u),
        Cell::I64(i) => json!(i),
        Cell::F32(f) => json!(f),
        Cell::F64(f) => json!(f),
        Cell::Numeric(n) => {
            // Convert PgNumeric to string representation
            json!(n.to_string())
        }
        Cell::Date(d) => json!(d.to_string()),
        Cell::Time(t) => json!(t.to_string()),
        Cell::Timestamp(ts) => json!(ts.to_string()),
        Cell::TimestampTz(ts) => json!(ts.to_rfc3339()),
        Cell::Uuid(u) => json!(u.to_string()),
        Cell::Json(j) => j.clone(),
        Cell::Bytes(b) => {
            // Encode bytes as base64
            json!(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                b
            ))
        }
        Cell::Array(arr) => {
            // For arrays, we'll serialize them as JSON arrays
            // This is a simplified implementation
            json!(format!("{:?}", arr))
        }
    }
}
