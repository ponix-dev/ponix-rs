use std::sync::Arc;

use async_trait::async_trait;
use casbin::{CoreApi, DefaultModel, Enforcer, MgmtApi, RbacApi};
use tokio::sync::RwLock;
use tokio_postgres_adapter::TokioPostgresAdapter;
use tracing::instrument;

use crate::auth::{
    base_policies, Action, AuthorizationProvider, OrgRole, Resource, RBAC_MODEL,
};
use crate::domain::{DomainError, DomainResult};
use crate::postgres::PostgresClient;

/// Thread-safe authorization service wrapping Casbin Enforcer with PostgreSQL storage
#[derive(Clone)]
pub struct PostgresAuthorizationService {
    enforcer: Arc<RwLock<Enforcer>>,
}

impl PostgresAuthorizationService {
    /// Create a new PostgresAuthorizationService with the given PostgreSQL client
    #[instrument(skip(postgres_client))]
    pub async fn new(postgres_client: &PostgresClient) -> DomainResult<Self> {
        // Create model from string
        let model = DefaultModel::from_str(RBAC_MODEL).await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to load model: {}", e))
        })?;

        // Create adapter with existing pool
        let pool = postgres_client.get_pool();
        let adapter = TokioPostgresAdapter::new_with_pool(pool).await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to create adapter: {}", e))
        })?;

        // Create enforcer
        let mut enforcer = Enforcer::new(model, adapter).await.map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to create enforcer: {}", e))
        })?;

        // Load existing policies from database
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
impl AuthorizationProvider for PostgresAuthorizationService {
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
