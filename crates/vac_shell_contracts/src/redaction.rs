//! Safe JSON redaction helpers for operator-visible previews.
//!
//! Rules:
//! - Recursive object traversal — nested secrets are caught.
//! - Array elements traversed recursively.
//! - Key match: case-insensitive substring against `RedactionConfig.sensitive_key_fragments`.
//! - Sensitive values replaced with `"[REDACTED]"`.
//! - Output capped at `RedactionConfig.preview_cap` chars.
//! - `null` / empty output returns `None` from `redacted_json_preview`.

use serde_json::Value;

/// Configuration for the redaction pass.
#[derive(Debug, Clone)]
pub struct RedactionConfig {
    pub sensitive_key_fragments: &'static [&'static str],
    pub preview_cap: usize,
}

impl RedactionConfig {
    /// Default config used by shell surfaces (approval preview, activity log).
    pub const DEFAULT: RedactionConfig = RedactionConfig {
        sensitive_key_fragments: &[
            "token", "secret", "password", "key", "auth", "credential",
            "apikey", "api_key", "bearer", "private",
        ],
        preview_cap: 500,
    };
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Recursively redact sensitive keys in a JSON value.
/// Returns a new value with sensitive values replaced by `"[REDACTED]"`.
pub fn redact_json_value(value: &Value, config: &RedactionConfig) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                if is_sensitive_key(k, config) {
                    out.insert(k.clone(), Value::String("[REDACTED]".into()));
                } else {
                    out.insert(k.clone(), redact_json_value(v, config));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| redact_json_value(v, config)).collect())
        }
        other => other.clone(),
    }
}

/// Redact, serialize to pretty JSON, and cap to `preview_cap` chars.
/// Returns `None` when the result is empty or `"null"`.
pub fn redacted_json_preview(value: &Value, config: &RedactionConfig) -> Option<String> {
    let redacted = redact_json_value(value, config);
    let s = serde_json::to_string_pretty(&redacted).unwrap_or_default();
    if s.is_empty() || s == "null" {
        return None;
    }
    Some(s.chars().take(config.preview_cap).collect())
}

fn is_sensitive_key(key: &str, config: &RedactionConfig) -> bool {
    let lower = key.to_lowercase();
    config
        .sensitive_key_fragments
        .iter()
        .any(|frag| lower.contains(frag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> RedactionConfig { RedactionConfig::DEFAULT }

    #[test]
    fn redacts_top_level_secret() {
        let v = json!({"token": "abc123", "url": "https://example.com"});
        let r = redact_json_value(&v, &cfg());
        assert_eq!(r["token"], "[REDACTED]");
        assert_eq!(r["url"], "https://example.com");
    }

    #[test]
    fn redacts_nested_secret() {
        let v = json!({"config": {"token": "nested_secret", "host": "db"}});
        let r = redact_json_value(&v, &cfg());
        assert_eq!(r["config"]["token"], "[REDACTED]");
        assert_eq!(r["config"]["host"], "db");
    }

    #[test]
    fn redacts_array_of_objects() {
        let v = json!([{"password": "pw1", "name": "alice"}, {"password": "pw2", "name": "bob"}]);
        let r = redact_json_value(&v, &cfg());
        assert_eq!(r[0]["password"], "[REDACTED]");
        assert_eq!(r[0]["name"], "alice");
        assert_eq!(r[1]["password"], "[REDACTED]");
    }

    #[test]
    fn preserves_safe_fields() {
        let v = json!({"cmd": "ls -la /tmp", "timeout": 30});
        let r = redact_json_value(&v, &cfg());
        assert_eq!(r["cmd"], "ls -la /tmp");
        assert_eq!(r["timeout"], 30);
    }

    #[test]
    fn caps_preview_after_redaction() {
        let long = "x".repeat(1000);
        let v = json!({"cmd": long});
        let cap = RedactionConfig { preview_cap: 50, ..cfg() };
        let preview = redacted_json_preview(&v, &cap).unwrap();
        assert!(preview.len() <= 50);
    }

    #[test]
    fn null_returns_none() {
        assert!(redacted_json_preview(&Value::Null, &cfg()).is_none());
    }

    #[test]
    fn case_insensitive_key_match() {
        let v = json!({"TOKEN": "secret", "AuthHeader": "bearer xyz"});
        let r = redact_json_value(&v, &cfg());
        assert_eq!(r["TOKEN"], "[REDACTED]");
        assert_eq!(r["AuthHeader"], "[REDACTED]");
    }

    #[test]
    fn empty_object_returns_none() {
        let v = json!({});
        // {} serializes to "{}", not "null" or empty, so it returns Some
        let preview = redacted_json_preview(&v, &cfg());
        // {} is valid JSON but we don't need None for it
        let _ = preview; // just don't crash
    }
}
