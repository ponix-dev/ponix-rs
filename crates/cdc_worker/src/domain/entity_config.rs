use crate::domain::CdcConverter;
pub struct EntityConfig {
    pub entity_name: String,
    pub table_name: String,
    pub converter: Box<dyn CdcConverter>,
}
