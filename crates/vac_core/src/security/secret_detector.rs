//! Secret Detection
//!
//! Detects sensitive information like API keys, IPs, AWS IDs

use regex::Regex;
use std::sync::OnceLock;

/// Types of secrets that can be detected
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretType {
    ApiKey,
    AwsAccessKey,
    AwsSecretKey,
    IpAddress,
    Email,
    Url,
    GenericSecret,
}

/// A detected secret
#[derive(Debug, Clone)]
pub struct DetectedSecret {
    pub secret_type: SecretType,
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// Secret detector using regex patterns
pub struct SecretDetector {
    patterns: Vec<(SecretType, Regex)>,
}

impl Default for SecretDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretDetector {
    pub fn new() -> Self {
        let patterns = vec![
            // AWS Access Key
            (
                SecretType::AwsAccessKey,
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            ),
            // AWS Secret Key (40 chars base64)
            (
                SecretType::AwsSecretKey,
                Regex::new(r#"(?i)aws.{0,20}['"][0-9a-zA-Z/+=]{40}['"]"#).unwrap(),
            ),
            // Generic API Key patterns
            (
                SecretType::ApiKey,
                Regex::new(r#"(?i)(api[_-]?key|apikey|apitoken)['""]?\s*[:=]\s*['"]?([a-zA-Z0-9_\-]{20,})"#).unwrap(),
            ),
            // IP Address
            (
                SecretType::IpAddress,
                Regex::new(r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b").unwrap(),
            ),
            // Email
            (
                SecretType::Email,
                Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
            ),
        ];

        Self { patterns }
    }

    /// Detect all secrets in text
    pub fn detect(&self, text: &str) -> Vec<DetectedSecret> {
        let mut secrets = Vec::new();

        for (secret_type, pattern) in &self.patterns {
            for mat in pattern.find_iter(text) {
                secrets.push(DetectedSecret {
                    secret_type: secret_type.clone(),
                    value: mat.as_str().to_string(),
                    start: mat.start(),
                    end: mat.end(),
                });
            }
        }

        // Sort by position
        secrets.sort_by_key(|s| s.start);
        secrets
    }

    /// Check if text contains any secrets
    pub fn contains_secrets(&self, text: &str) -> bool {
        self.patterns
            .iter()
            .any(|(_, pattern)| pattern.is_match(text))
    }
}

/// Global detector instance
static DETECTOR: OnceLock<SecretDetector> = OnceLock::new();

/// Get global detector instance
pub fn get_detector() -> &'static SecretDetector {
    DETECTOR.get_or_init(SecretDetector::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_aws_key() {
        let detector = SecretDetector::new();
        let text = "AWS_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE";
        let secrets = detector.detect(text);
        assert!(!secrets.is_empty());
    }

    #[test]
    fn test_detect_ip() {
        let detector = SecretDetector::new();
        let text = "Server at 192.168.1.1";
        let secrets = detector.detect(text);
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].secret_type, SecretType::IpAddress);
    }

    #[test]
    fn test_detect_email() {
        let detector = SecretDetector::new();
        let text = "Contact: user@example.com";
        let secrets = detector.detect(text);
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].secret_type, SecretType::Email);
    }
}
