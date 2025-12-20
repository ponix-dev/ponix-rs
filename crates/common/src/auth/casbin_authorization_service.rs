use std::sync::Arc;

use async_trait::async_trait;
use casbin::{Adapter, CoreApi, DefaultModel, Enforcer, MgmtApi, RbacApi};
use tokio::sync::RwLock;
use tracing::instrument;

use crate::auth::{base_policies, Action, AuthorizationProvider, OrgRole, Resource, RBAC_MODEL};
use crate::domain::{DomainError, DomainResult};

/// Thread-safe authorization service wrapping Casbin Enforcer.
///
/// This service is storage-agnostic - it accepts any Casbin adapter
/// (PostgreSQL, file, memory, etc.) and provides authorization operations.
#[derive(Clone)]
pub struct CasbinAuthorizationService {
    enforcer: Arc<RwLock<Enforcer>>,
}

impl CasbinAuthorizationService {
    /// Create a new CasbinAuthorizationService with the given adapter.
    ///
    /// The adapter can be any implementation of `casbin::Adapter` (PostgreSQL, file, memory, etc.).
    #[instrument(skip(adapter))]
    pub async fn new<A>(adapter: A) -> DomainResult<Self>
    where
        A: Adapter + 'static,
    {
        // Create model from string
        let model = DefaultModel::from_str(RBAC_MODEL).await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to load model: {}", e))
        })?;

        // Create enforcer with the provided adapter
        let mut enforcer = Enforcer::new(model, adapter).await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to create enforcer: {}", e))
        })?;

        // Load existing policies from the adapter's storage
        enforcer.load_policy().await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to load policies: {}", e))
        })?;

        // Add base policies if not already present
        for policy in base_policies() {
            // Ignore errors for existing policies (unique constraint)
            let _ = enforcer.add_policy(policy).await;
        }

        Ok(Self {
            enforcer: Arc::new(RwLock::new(enforcer)),
        })
    }
}

#[async_trait]
impl AuthorizationProvider for CasbinAuthorizationService {
    #[instrument(skip(self))]
    async fn check_permission(
        &self,
        user_id: &str,
        org_id: &str,
        resource: Resource,
        action: Action,
    ) -> DomainResult<bool> {
        let enforcer = self.enforcer.read().await;
        let result = enforcer
            .enforce((user_id, org_id, resource.as_str(), action.as_str()))
            .map_err(|e| DomainError::AuthorizationError(format!("Enforcement error: {}", e)))?;
        Ok(result)
    }

    #[instrument(skip(self))]
    async fn require_permission(
        &self,
        user_id: &str,
        org_id: &str,
        resource: Resource,
        action: Action,
    ) -> DomainResult<()> {
        if !self
            .check_permission(user_id, org_id, resource, action)
            .await?
        {
            return Err(DomainError::PermissionDenied(format!(
                "User {} does not have permission to {} {} in org {}",
                user_id,
                action.as_str(),
                resource.as_str(),
                org_id
            )));
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn assign_role(&self, user_id: &str, org_id: &str, role: OrgRole) -> DomainResult<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer
            .add_grouping_policy(vec![
                user_id.to_string(),
                role.as_str().to_string(),
                org_id.to_string(),
            ])
            .await
            .map_err(|e| {
                DomainError::AuthorizationError(format!("Failed to assign role: {}", e))
            })?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn remove_role(&self, user_id: &str, org_id: &str, role: OrgRole) -> DomainResult<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer
            .remove_grouping_policy(vec![
                user_id.to_string(),
                role.as_str().to_string(),
                org_id.to_string(),
            ])
            .await
            .map_err(|e| {
                DomainError::AuthorizationError(format!("Failed to remove role: {}", e))
            })?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_roles_for_user_in_org(
        &self,
        user_id: &str,
        org_id: &str,
    ) -> DomainResult<Vec<String>> {
        let enforcer = self.enforcer.read().await;
        let roles = enforcer.get_roles_for_user(user_id, Some(org_id));
        Ok(roles)
    }

    #[instrument(skip(self))]
    async fn assign_super_admin(&self, user_id: &str) -> DomainResult<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer
            .add_grouping_policy(vec![
                user_id.to_string(),
                "super_admin".to_string(),
                "*".to_string(),
            ])
            .await
            .map_err(|e| {
                DomainError::AuthorizationError(format!("Failed to assign super admin: {}", e))
            })?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn is_super_admin(&self, user_id: &str) -> DomainResult<bool> {
        let enforcer = self.enforcer.read().await;
        let roles = enforcer.get_roles_for_user(user_id, Some("*"));
        Ok(roles.contains(&"super_admin".to_string()))
    }
}
