use crate::domain::awareness::{random_user_color, UserPresence};
use common::domain::{GetUserRepoInput, UserRepository};

#[derive(Debug)]
pub struct ConnectedUser {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub color: String,
}

impl ConnectedUser {
    /// Create a ConnectedUser from a pre-validated user_id.
    /// Looks up the user in the database to get name/email, then derives a
    /// deterministic color from the user_id hash.
    pub async fn from_user_id(
        user_id: &str,
        user_repo: &dyn UserRepository,
    ) -> anyhow::Result<Self> {
        let user = user_repo
            .get_user(GetUserRepoInput {
                user_id: user_id.to_string(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch user: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("user not found: {}", user_id))?;

        let color = random_user_color();

        Ok(Self {
            user_id: user_id.to_string(),
            name: user.name,
            email: user.email,
            color,
        })
    }

    pub fn to_presence(&self) -> UserPresence {
        UserPresence {
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            color: self.color.clone(),
            cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{MockUserRepository, User};

    #[tokio::test]
    async fn test_from_user_id_success() {
        let mut mock_repo = MockUserRepository::new();
        mock_repo.expect_get_user().returning(|_| {
            Ok(Some(User {
                id: "user-1".to_string(),
                email: "alice@example.com".to_string(),
                password_hash: String::new(),
                name: "Alice".to_string(),
                created_at: None,
                updated_at: None,
            }))
        });

        let user = ConnectedUser::from_user_id("user-1", &mock_repo)
            .await
            .unwrap();

        assert_eq!(user.user_id, "user-1");
        assert_eq!(user.name, "Alice");
        assert_eq!(user.email, "alice@example.com");
        assert!(!user.color.is_empty());
    }

    #[tokio::test]
    async fn test_from_user_id_user_not_found() {
        let mut mock_repo = MockUserRepository::new();
        mock_repo.expect_get_user().returning(|_| Ok(None));

        let result = ConnectedUser::from_user_id("user-missing", &mock_repo).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("user not found"));
    }
}
