use chrono::{DateTime, Utc};

/// Organization domain entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// External input for creating an organization (no ID)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOrganizationInput {
    pub name: String,
}

/// Internal input with generated ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOrganizationInputWithId {
    pub id: String,
    pub name: String,
}

/// Input for getting an organization by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetOrganizationInput {
    pub organization_id: String,
}

/// Input for updating an organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOrganizationInput {
    pub organization_id: String,
    pub name: String,
}

/// Input for deleting an organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOrganizationInput {
    pub organization_id: String,
}

/// Input for listing organizations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListOrganizationsInput {}
