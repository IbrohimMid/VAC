//! Secret redaction — prevent leaking sensitive data in traces/checkpoints.

use once_cell::sync::Lazy;
use regex::Regex;

#[allow(clippy::expect_used)] // Compile-time-constant regexes; panic at init is intentional.
static SECRET_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        // AWS keys - replace entire match
        (Regex::new(r"AKIA[0-9A-Z]{16}")
            .expect("redaction: failed to compile aws_key regex"), "[REDACTED]"),
        // API keys - keep prefix, redact value
        (Regex::new(r#"(?i)(api[_-]?key|apikey|access[_-]?token|secret[_-]?key)["']?\s*[:=]\s*["']?([a-zA-Z0-9_\-]{20,})"#)
            .expect("redaction: failed to compile api_key regex"), "$1=[REDACTED]"),
        // Bearer tokens
        (Regex::new(r#"(?i)(bearer\s+)([a-zA-Z0-9_\-\.]{20,})"#)
            .expect("redaction: failed to compile bearer regex"), "$1[REDACTED]"),
        // Passwords
        (Regex::new(r#"(?i)(password|passwd|pwd)["']?\s*[:=]\s*["']?([^\s"']{8,})"#)
            .expect("redaction: failed to compile password regex"), "$1=[REDACTED]"),
    ]
});

/// Redact secrets from text, replacing with [REDACTED].
pub fn redact_secrets(text: &str) -> String {
    let mut result = text.to_string();

    for (pattern, replacement) in SECRET_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }

    result
}

fn is_sensitive_key(k: &str) -> bool {
    matches!(
        k,
        "api_key"
            | "apikey"
            | "secret"
            | "password"
            | "token"
            | "authorization"
            | "auth"
            | "credential"
            | "private_key"
            | "access_key"
            | "secret_key"
            | "signing_key"
    )
}

/// Redact secrets from JSON value.
pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = redact_secrets(s);
        }
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_sensitive_key(&key.to_lowercase()) {
                    *val = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json(val);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_json(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_keys() {
        let text = "api_key=sk_test_1234567890abcdefghij";
        let redacted = redact_secrets(text);
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("sk_test"));
    }

    #[test]
    fn redacts_aws_keys() {
        let text = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let redacted = redact_secrets(text);
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn redacts_bearer_tokens() {
        let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let redacted = redact_secrets(text);
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn all_redaction_patterns_compile() {
        // Force Lazy evaluation and verify all patterns compiled
        assert_eq!(SECRET_PATTERNS.len(), 4, "expected 4 redaction patterns");
    }

    #[test]
    fn redacts_json_keys() {
        let mut json = serde_json::json!({
            "api_key": "secret123",
            "data": "public",
            "public_key": "not_redacted",
            "keyboard_layout": "us",
            "nested": {
                "password": "pass456"
            }
        });
        redact_json(&mut json);
        assert_eq!(json["api_key"], "[REDACTED]");
        assert_eq!(json["data"], "public");
        assert_eq!(json["public_key"], "not_redacted");
        assert_eq!(json["keyboard_layout"], "us");
        assert_eq!(json["nested"]["password"], "[REDACTED]");
    }
}
