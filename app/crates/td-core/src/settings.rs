use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::layout::Orientation;

pub const IDLE_TIMEOUT_MIN_S: u32 = 1;
pub const IDLE_TIMEOUT_MAX_S: u32 = 86400;

/// The deck settings stored in `/storage/settings.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub idle_timeout_s: u32,
    pub orientation: Orientation,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            idle_timeout_s: 600,
            orientation: Orientation::Landscape,
        }
    }
}

impl Settings {
    /// Reads settings the way the firmware does: an unknown field is ignored, and a
    /// missing or out-of-range field takes its default.
    pub fn from_value(value: &Value) -> Settings {
        let defaults = Settings::default();
        let idle_timeout_s = value
            .get("idle_timeout_s")
            .and_then(Value::as_u64)
            .and_then(|timeout| u32::try_from(timeout).ok())
            .filter(|timeout| (IDLE_TIMEOUT_MIN_S..=IDLE_TIMEOUT_MAX_S).contains(timeout))
            .unwrap_or(defaults.idle_timeout_s);
        let orientation = match value.get("orientation").and_then(Value::as_str) {
            Some("portrait") => Orientation::Portrait,
            _ => Orientation::Landscape,
        };
        Settings {
            idle_timeout_s,
            orientation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn firmware_parsing_rules() {
        assert_eq!(Settings::from_value(&json!({})), Settings::default());
        assert_eq!(
            Settings::from_value(
                &json!({"idle_timeout_s": 120, "orientation": "portrait", "extra": 1})
            ),
            Settings {
                idle_timeout_s: 120,
                orientation: Orientation::Portrait
            }
        );
        assert_eq!(
            Settings::from_value(&json!({"idle_timeout_s": 0})).idle_timeout_s,
            600
        );
        assert_eq!(
            Settings::from_value(&json!({"idle_timeout_s": 86401})).idle_timeout_s,
            600
        );
        assert_eq!(
            Settings::from_value(&json!({"orientation": "sideways"})).orientation,
            Orientation::Landscape
        );
    }
}
