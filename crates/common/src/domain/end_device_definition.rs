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

/// Domain representation of an End Device Definition
/// Contains ordered payload contracts for processing device data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndDeviceDefinition {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub contracts: Vec<PayloadContract>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a definition (domain service -> repository)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateEndDeviceDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub contracts: Vec<PayloadContract>,
}

/// Repository input for retrieving a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetEndDeviceDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
}

/// Repository input for updating a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateEndDeviceDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
    pub name: Option<String>,
    pub contracts: Option<Vec<PayloadContract>>,
}

/// Repository input for deleting a definition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteEndDeviceDefinitionRepoInput {
    pub id: String,
    pub organization_id: String,
}

/// Repository input for listing definitions by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEndDeviceDefinitionsRepoInput {
    pub organization_id: String,
}

/// Repository trait for end device definition storage operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait EndDeviceDefinitionRepository: Send + Sync {
    /// Create a new definition
    async fn create_definition(
        &self,
        input: CreateEndDeviceDefinitionRepoInput,
    ) -> DomainResult<EndDeviceDefinition>;

    /// Get a definition by ID
    async fn get_definition(
        &self,
        input: GetEndDeviceDefinitionRepoInput,
    ) -> DomainResult<Option<EndDeviceDefinition>>;

    /// Update an existing definition
    async fn update_definition(
        &self,
        input: UpdateEndDeviceDefinitionRepoInput,
    ) -> DomainResult<EndDeviceDefinition>;

    /// Delete a definition (will fail if devices reference it)
    async fn delete_definition(
        &self,
        input: DeleteEndDeviceDefinitionRepoInput,
    ) -> DomainResult<()>;

    /// List all definitions for an organization
    async fn list_definitions(
        &self,
        input: ListEndDeviceDefinitionsRepoInput,
    ) -> DomainResult<Vec<EndDeviceDefinition>>;
}
