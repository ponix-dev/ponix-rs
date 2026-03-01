use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

/// A single payload processing contract: match -> transform -> validate
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadContract {
    pub match_expression: String,
    pub transform_expression: String,
    pub json_schema: String,
    #[serde(with = "base64_bytes")]
    pub compiled_match: Vec<u8>,
    #[serde(with = "base64_bytes")]
    pub compiled_transform: Vec<u8>,
}

/// Domain representation of a Data Stream Definition
/// Contains ordered payload contracts for processing data stream data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStreamDefinition {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub contracts: Vec<PayloadContract>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a definition (domain service -> repository)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDataStreamDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub contracts: Vec<PayloadContract>,
}

/// Repository input for retrieving a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetDataStreamDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
}

/// Repository input for updating a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateDataStreamDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
    pub name: Option<String>,
    pub contracts: Option<Vec<PayloadContract>>,
}

/// Repository input for deleting a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteDataStreamDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
}

/// Repository input for listing definitions by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDataStreamDefinitionsRepoInput {
    pub organization_id: String,
}

/// Repository trait for data stream definition storage operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait DataStreamDefinitionRepository: Send + Sync {
    /// Create a new definition
    async fn create_definition(
        &self,
        input: CreateDataStreamDefinitionRepoInput,
    ) -> DomainResult<DataStreamDefinition>;

    /// Get a definition by ID
    async fn get_definition(
        &self,
        input: GetDataStreamDefinitionRepoInput,
    ) -> DomainResult<Option<DataStreamDefinition>>;

    /// Update an existing definition
    async fn update_definition(
        &self,
        input: UpdateDataStreamDefinitionRepoInput,
    ) -> DomainResult<DataStreamDefinition>;

    /// Delete a definition (will fail if data streams reference it)
    async fn delete_definition(
        &self,
        input: DeleteDataStreamDefinitionRepoInput,
    ) -> DomainResult<()>;

    /// List all definitions for an organization
    async fn list_definitions(
        &self,
        input: ListDataStreamDefinitionsRepoInput,
    ) -> DomainResult<Vec<DataStreamDefinition>>;
}
