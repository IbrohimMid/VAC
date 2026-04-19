//! Redaction engine — strips sensitive data from trace records.

use regex::Regex;
use serde_json::Value;

pub struct RedactionEngine {
    patterns: Vec<Regex>,
    /// Pre-compiled path regex; `None` when `strip_paths` is false.
    path_re: Option<Regex>,
}

impl RedactionEngine {
    /// Build a `RedactionEngine`.
    ///
    /// `custom_patterns` are additional regex strings supplied by the caller.
    /// Returns `Err` if any custom pattern fails to compile (built-in patterns
    /// are compile-time constants and would only panic on a Rust bug).
    pub fn new(strip_paths: bool, custom_patterns: &[String]) -> Self {
        #[allow(clippy::unwrap_used)] // Compile-time-constant regexes; infallible.
        let mut patterns = vec![
            Regex::new(r"(?i)(sk-[a-zA-Z0-9]{20,})").unwrap(),
            Regex::new(r"(?i)(api[_-]?key\s*[:=]\s*['\x22]?[a-zA-Z0-9]{16,})").unwrap(),
            Regex::new(r"(?i)(bearer\s+[a-zA-Z0-9._-]{20,})").unwrap(),
        ];

        for pattern in custom_patterns {
            match Regex::new(pattern) {
                Ok(re) => patterns.push(re),
                Err(e) => {
                    tracing::warn!(pattern, error = %e, "custom redaction pattern failed to compile — skipped");
                }
            }
        }

        // Compile path regex once at construction time, not on every call.
        // Pattern: Unix absolute paths starting with a known root prefix, not
        // inside URL schemes (no preceding `://`).
        #[allow(clippy::unwrap_used)] // Compile-time-constant pattern.
        let path_re = if strip_paths {
            Some(
                Regex::new(
                    r"(?:^|[^:/\w])(/(?:home|usr|var|tmp|opt|root|etc|proc|run|srv|mnt)/[A-Za-z0-9_./-]+)",
                )
                .unwrap(),
            )
        } else {
            None
        };

        Self { patterns, path_re }
    }

    pub fn redact_string(&self, input: &str) -> String {
        let mut result = input.to_string();

        for pattern in &self.patterns {
            result = pattern.replace_all(&result, "[REDACTED]").into_owned();
        }

        if let Some(path_re) = &self.path_re {
            // Replace only the captured path group (group 1), preserving the
            // non-path anchor character before it.
            result = path_re
                .replace_all(&result, |caps: &regex::Captures<'_>| {
                    let full = caps.get(0).map_or("", |m| m.as_str());
                    let path = caps.get(1).map_or("", |m| m.as_str());
                    full.replacen(path, "[PATH]", 1)
                })
                .into_owned();
        }

        result
    }

    pub fn redact_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.redact_string(s)),
            Value::Array(arr) => Value::Array(arr.iter().map(|v| self.redact_value(v)).collect()),
            Value::Object(map) => {
                let redacted: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.redact_value(v)))
                    .collect();
                Value::Object(redacted)
            }
            other => other.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_string_masks_secrets_and_paths() {
        let engine = RedactionEngine::new(true, &[]);
        let redacted = engine.redact_string("token sk-abc12345678901234567 path /tmp/project/file");

        assert!(redacted.contains("[REDACTED]"), "secret must be redacted");
        assert!(redacted.contains("[PATH]"), "path must be redacted");
        assert!(!redacted.contains("sk-abc12345678901234567"), "raw secret must not appear");
    }

    #[test]
    fn strips_unix_path_not_url() {
        let engine = RedactionEngine::new(true, &[]);
        let input = "log at /home/alice/app.log via https://api.example.com/v1/resource";
        let redacted = engine.redact_string(input);

        assert!(redacted.contains("[PATH]"), "unix path must be stripped");
        assert!(
            redacted.contains("https://api.example.com/v1/resource"),
            "URL must be preserved: got {redacted}"
        );
        assert!(!redacted.contains("/home/alice/app.log"), "raw path must not appear");
    }

    #[test]
    fn does_not_strip_when_strip_paths_false() {
        let engine = RedactionEngine::new(false, &[]);
        let input = "/home/alice/app.log";
        assert_eq!(engine.redact_string(input), input, "paths must not be touched");
    }

    #[test]
    fn bad_custom_pattern_is_skipped_not_panicked() {
        // An invalid regex must not panic — it should be silently skipped.
        let engine = RedactionEngine::new(false, &["[invalid".to_string()]);
        // Only built-in patterns; arbitrary input passes through.
        assert_eq!(engine.redact_string("hello"), "hello");
    }

    #[test]
    fn redact_value_walks_nested_json() {
        let engine = RedactionEngine::new(false, &[]);
        let value = json!({
            "nested": [
                "Bearer abcdefghijklmnopqrstuvwxyz",
                { "path": "/home/user/project/" }
            ]
        });

        let redacted = engine.redact_value(&value);
        let redacted_text = serde_json::to_string(&redacted).unwrap();

        assert!(redacted_text.contains("[REDACTED]"));
        // strip_paths=false → paths untouched
        assert!(redacted_text.contains("/home/user/project/"));
    }
}
