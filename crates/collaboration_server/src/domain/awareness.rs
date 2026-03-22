use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<serde_json::Value>,
}

pub fn random_user_color() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let hue = rng.gen_range(0.0..360.0);
    let sat = rng.gen_range(50.0..80.0);
    let lit = rng.gen_range(40.0..65.0);
    hsl_to_hex(hue, sat, lit)
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
    fn test_random_user_color_valid_hex() {
        for _ in 0..10 {
            let color = random_user_color();
            assert!(
                color.len() == 7 && color.starts_with('#'),
                "Expected #rrggbb format, got: {}",
                color
            );
            assert!(color[1..].chars().all(|c| c.is_ascii_hexdigit()));
        }
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
