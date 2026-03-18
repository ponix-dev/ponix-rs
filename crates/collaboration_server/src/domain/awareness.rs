use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<serde_json::Value>,
}

pub fn derive_user_color(user_id: &str) -> String {
    let hash = Sha256::digest(user_id.as_bytes());
    let hue = u16::from_be_bytes([hash[0], hash[1]]) % 360;
    let sat = 55.0 + (hash[2] as f64 / 255.0) * 20.0; // 55-75%
    let lit = 45.0 + (hash[3] as f64 / 255.0) * 20.0; // 45-65%
    hsl_to_hex(hue as f64, sat, lit)
}

fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let s = s / 100.0;
    let l = l / 100.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r + m) * 255.0).round() as u8;
    let g = ((g + m) * 255.0).round() as u8;
    let b = ((b + m) * 255.0).round() as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_user_color_deterministic() {
        let color1 = derive_user_color("user-123");
        let color2 = derive_user_color("user-123");
        assert_eq!(color1, color2);
    }

    #[test]
    fn test_derive_user_color_different_users() {
        let color1 = derive_user_color("user-abc");
        let color2 = derive_user_color("user-xyz");
        assert_ne!(color1, color2);
    }

    #[test]
    fn test_derive_user_color_valid_hex() {
        let color = derive_user_color("test-user");
        assert!(
            color.len() == 7 && color.starts_with('#'),
            "Expected #rrggbb format, got: {}",
            color
        );
        // All chars after '#' should be hex digits
        assert!(color[1..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_user_presence_json_roundtrip() {
        let cursor_value = serde_json::json!({
            "anchor": {"type": {"client": 1, "clock": 0}, "tname": null, "item": {"client": 1, "clock": 5}},
            "focus": {"type": {"client": 1, "clock": 0}, "tname": null, "item": {"client": 1, "clock": 8}}
        });
        let presence = UserPresence {
            user_id: "user-1".to_string(),
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            color: "#ff0000".to_string(),
            cursor: Some(cursor_value.clone()),
        };

        let json = serde_json::to_string(&presence).unwrap();
        let decoded: UserPresence = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.user_id, "user-1");
        assert_eq!(decoded.cursor, Some(cursor_value));
    }

    #[test]
    fn test_user_presence_json_no_cursor() {
        let presence = UserPresence {
            user_id: "user-1".to_string(),
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            color: "#ff0000".to_string(),
            cursor: None,
        };

        let json = serde_json::to_string(&presence).unwrap();
        assert!(
            !json.contains("cursor"),
            "cursor field should be omitted when None"
        );
    }
}
