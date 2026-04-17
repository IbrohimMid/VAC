//! Redaction engine — strips sensitive data from trace records.

use regex::Regex;
use serde_json::Value;

pub trait RedactionPolicy {
    fn redact_string(&self, input: &str) -> String;
    fn redact_value(&self, value: &Value) -> Value;
}

pub struct RedactionEngine {
    patterns: Vec<Regex>,
    strip_paths: bool,
}

impl RedactionEngine {
    pub fn new(strip_paths: bool, custom_patterns: &[String]) -> Self {
        let mut patterns = vec![
            Regex::new(r"(?i)(sk-[a-zA-Z0-9]{20,})").unwrap(),
            Regex::new(r"(?i)(api[_-]?key\s*[:=]\s*['\x22]?[a-zA-Z0-9]{16,}['\x22]?)").unwrap(),
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
}

impl RedactionPolicy for RedactionEngine {
    fn redact_string(&self, input: &str) -> String {
        let mut result = input.to_string();
        for pattern in &self.patterns {
            result = pattern.replace_all(&result, "[REDACTED]").to_string();
        }
        if self.strip_paths {
            let path_re = Regex::new(r"(?P<p>/(?:[a-zA-Z0-9_-]+/)+)").unwrap();
            result = path_re.replace_all(&result, "[PATH]/").to_string();
        }
        result
    }

    fn redact_value(&self, value: &Value) -> Value {
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

    #[test]
    fn test_redaction_corpus() {
        let engine = RedactionEngine::new(true, &[]);

        let corpus = vec![
            ("Bearer abcdefghijklmnopqrstuvwxyz", "[REDACTED]"),
            ("api_key='1234567890abcdef'", "[REDACTED]"),
            ("sk-1234567890abcdefghij", "[REDACTED]"),
            (
                "Path is /home/user/workspace/file.txt",
                "Path is [PATH]/file.txt",
            ),
            ("No sensitive data here", "No sensitive data here"),
        ];

        for (input, expected) in corpus {
            assert_eq!(engine.redact_string(input), expected);
        }
    }

    #[test]
    fn test_redaction_json() {
        let engine = RedactionEngine::new(true, &[]);
        let input = serde_json::json!({
            "key": "sk-1234567890abcdefghij",
            "nested": {
                "auth": "Bearer abcdefghijklmnopqrstuvwxyz",
                "path": "/var/log/syslog"
            },
            "array": ["api_key=1234567890abcdef"]
        });

        let expected = serde_json::json!({
            "key": "[REDACTED]",
            "nested": {
                "auth": "[REDACTED]",
                "path": "[PATH]/syslog"
            },
            "array": ["[REDACTED]"]
        });

        assert_eq!(engine.redact_value(&input), expected);
    }
}
