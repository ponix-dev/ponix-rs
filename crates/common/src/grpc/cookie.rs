use tonic::metadata::{MetadataMap, MetadataValue};

/// Cookie configuration for refresh tokens
pub struct RefreshTokenCookie {
    pub value: String,
    pub max_age_seconds: i64,
    pub path: String,
    pub secure: bool,
}

impl RefreshTokenCookie {
    /// Create a new refresh token cookie
    pub fn new(value: String, expiration_days: u64, secure: bool) -> Self {
        Self {
            value,
            max_age_seconds: (expiration_days * 24 * 60 * 60) as i64,
            path: "/user.v1.UserService".to_string(),
            secure,
        }
    }

    /// Create a cookie that clears the refresh token (for logout)
    pub fn clear(secure: bool) -> Self {
        Self {
            value: String::new(),
            max_age_seconds: 0,
            path: "/user.v1.UserService".to_string(),
            secure,
        }
    }

    /// Build the Set-Cookie header value
    pub fn to_header_value(&self) -> String {
        let mut parts = vec![
            format!("refresh_token={}", self.value),
            format!("Max-Age={}", self.max_age_seconds),
            format!("Path={}", self.path),
            "HttpOnly".to_string(),
            "SameSite=None".to_string(),
        ];

        if self.secure {
            parts.push("Secure".to_string());
        }

        parts.join("; ")
    }

    /// Add the Set-Cookie header to response metadata
    pub fn add_to_metadata(&self, metadata: &mut MetadataMap) {
        if let Ok(value) = self.to_header_value().parse::<MetadataValue<_>>() {
            metadata.insert("set-cookie", value);
        }
    }
}

/// Extract refresh token from request cookies
pub fn extract_refresh_token_from_cookies(metadata: &MetadataMap) -> Option<String> {
    metadata
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie_str| {
            cookie_str
                .split(';')
                .map(|s| s.trim())
                .find(|s| s.starts_with("refresh_token="))
                .map(|s| s.trim_start_matches("refresh_token=").to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_new() {
        let cookie = RefreshTokenCookie::new("my-token".to_string(), 7, true);
        assert_eq!(cookie.value, "my-token");
        assert_eq!(cookie.max_age_seconds, 7 * 24 * 60 * 60);
        assert_eq!(cookie.path, "/user.v1.UserService");
        assert!(cookie.secure);
    }

    #[test]
    fn test_cookie_clear() {
        let cookie = RefreshTokenCookie::clear(true);
        assert_eq!(cookie.value, "");
        assert_eq!(cookie.max_age_seconds, 0);
    }

    #[test]
    fn test_to_header_value_secure() {
        let cookie = RefreshTokenCookie::new("token123".to_string(), 7, true);
        let header = cookie.to_header_value();
        assert!(header.contains("refresh_token=token123"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("Secure"));
        assert!(header.contains("SameSite=None"));
    }

    #[test]
    fn test_to_header_value_insecure() {
        let cookie = RefreshTokenCookie::new("token123".to_string(), 7, false);
        let header = cookie.to_header_value();
        assert!(header.contains("refresh_token=token123"));
        assert!(header.contains("HttpOnly"));
        assert!(!header.contains("Secure"));
    }

    #[test]
    fn test_extract_refresh_token_from_cookies() {
        let mut metadata = MetadataMap::new();
        metadata.insert(
            "cookie",
            "refresh_token=abc123; other=value".parse().unwrap(),
        );
        assert_eq!(
            extract_refresh_token_from_cookies(&metadata),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_refresh_token_from_cookies_not_found() {
        let mut metadata = MetadataMap::new();
        metadata.insert("cookie", "other=value".parse().unwrap());
        assert_eq!(extract_refresh_token_from_cookies(&metadata), None);
    }

    #[test]
    fn test_extract_refresh_token_from_cookies_no_cookie_header() {
        let metadata = MetadataMap::new();
        assert_eq!(extract_refresh_token_from_cookies(&metadata), None);
    }
}
