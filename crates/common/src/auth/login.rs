/// Input for user login
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUserInput {
    pub email: String,
    pub password: String,
}

/// Output from successful login
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUserOutput {
    pub access_token: String,
    pub refresh_token: String,
}

/// Input for refreshing a token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshTokenInput {
    pub refresh_token: String,
}

/// Output from successful token refresh
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshTokenOutput {
    pub access_token: String,
    pub refresh_token: String,
}

/// Input for logout
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogoutInput {
    pub refresh_token: String,
}
