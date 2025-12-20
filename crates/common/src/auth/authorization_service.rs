use async_trait::async_trait;

use crate::auth::{Action, OrgRole, Resource};
use crate::domain::DomainResult;

/// Trait for authorization operations
/// Enables mocking in tests while using Casbin in production
#[async_trait]
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait AuthorizationProvider: Send + Sync {
    /// Check if a user has permission to perform an action on a resource in an organization
    async fn check_permission(
        &self,
        user_id: &str,
        org_id: &str,
        resource: Resource,
        action: Action,
    ) -> DomainResult<bool>;

    /// Check permission and return PermissionDenied error if not allowed
    async fn require_permission(
        &self,
        user_id: &str,
        org_id: &str,
        resource: Resource,
        action: Action,
    ) -> DomainResult<()>;

    /// Assign a role to a user for a specific organization
    async fn assign_role(&self, user_id: &str, org_id: &str, role: OrgRole) -> DomainResult<()>;

    /// Remove a role from a user for a specific organization
    async fn remove_role(&self, user_id: &str, org_id: &str, role: OrgRole) -> DomainResult<()>;

    /// Get all roles for a user in a specific organization
    async fn get_roles_for_user_in_org(
        &self,
        user_id: &str,
        org_id: &str,
    ) -> DomainResult<Vec<String>>;

    /// Assign super admin role to a user (cross-org access)
    async fn assign_super_admin(&self, user_id: &str) -> DomainResult<()>;

    /// Check if a user is a super admin
    async fn is_super_admin(&self, user_id: &str) -> DomainResult<bool>;
}
