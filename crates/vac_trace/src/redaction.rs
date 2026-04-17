//! Redaction engine — strips sensitive data from trace records.

use regex::Regex;
use serde_json::Value;

pub struct RedactionEngine {
    patterns: Vec<Regex>,
    strip_paths: bool,
}

impl RedactionEngine {
    pub fn new(strip_paths: bool, custom_patterns: &[String]) -> Self {
        let mut patterns = vec![
            Regex::new(r"(?i)(sk-[a-zA-Z0-9]{20,})").unwrap(),
            Regex::new(r"(?i)(api[_-]?key\s*[:=]\s*['\x22]?[a-zA-Z0-9]{16,})").unwrap(),
            Regex::new(r"(?i)(bearer\s+[a-zA-Z0-9._-]{20,})").unwrap(),
        ];

        for pattern in custom_patterns {
            if let Ok(re) = Regex::new(pattern) {
                patterns.push(re);
            }
        }

        Self {
            patterns,
            strip_paths,
        }
    }

    pub fn redact_string(&self, input: &str) -> String {
        let mut result = input.to_string();
        for pattern in &self.patterns {
            result = pattern.replace_all(&result, "[REDACTED]").to_string();
        }
        if self.strip_paths {
            let path_re = Regex::new(r"/[a-zA-Z0-9/_.-]+/").unwrap();
            result = path_re.replace_all(&result, "[PATH]/").to_string();
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_string_masks_secrets_and_paths() {
        let engine = RedactionEngine::new(true, &[]);
        let redacted = engine.redact_string("token sk-abc12345678901234567 path /tmp/project/");

        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("[PATH]/"));
        assert!(!redacted.contains("sk-abc12345678901234567"));
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
        assert!(redacted_text.contains("/home/user/project/"));
    }
}
