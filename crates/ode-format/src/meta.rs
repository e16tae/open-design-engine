use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub format_version: String,
    pub generator: String,
    pub created_at: String,
    pub modified_at: String,
}

impl Meta {
    pub fn new(generator: &str) -> Self {
        let now = now_iso8601();
        Self {
            format_version: "1.0.0".to_string(),
            generator: generator.to_string(),
            created_at: now.clone(),
            modified_at: now,
        }
    }

    /// Create Meta with defaults for legacy .ode.json files.
    pub fn legacy() -> Self {
        let now = now_iso8601();
        Self {
            format_version: "1.0.0".to_string(),
            generator: "ode-format (legacy)".to_string(),
            created_at: now.clone(),
            modified_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.modified_at = now_iso8601();
    }
}

fn now_iso8601() -> String {
    // Simple UTC timestamp without external crate.
    // NOTE: Approximate date (ignores leap years, assumes 30-day months).
    // Acceptable for metadata timestamps — exact dates are not critical.
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let (s, m, h) = (secs % 60, (secs / 60) % 60, (secs / 3600) % 24);
    let days = secs / 86400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + days / 365,
        ((days % 365) / 30).min(11) + 1,
        (days % 365) % 30 + 1,
        h, m, s
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_new_has_correct_defaults() {
        let meta = Meta::new("ode-cli 0.1.0");
        assert_eq!(meta.format_version, "1.0.0");
        assert_eq!(meta.generator, "ode-cli 0.1.0");
        assert!(!meta.created_at.is_empty());
        assert!(!meta.modified_at.is_empty());
    }

    #[test]
    fn meta_roundtrip_json() {
        let meta = Meta::new("ode-cli 0.1.0");
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: Meta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta.format_version, parsed.format_version);
        assert_eq!(meta.generator, parsed.generator);
    }

    #[test]
    fn meta_legacy_defaults() {
        let meta = Meta::legacy();
        assert_eq!(meta.generator, "ode-format (legacy)");
    }
}
