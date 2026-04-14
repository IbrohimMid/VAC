//! Secret Substitution Engine
//!
//! Replaces secrets with placeholders and restores them

use super::secret_detector::{DetectedSecret, SecretDetector};
use std::collections::HashMap;

/// A placeholder for a secret
#[derive(Debug, Clone)]
pub struct SecretPlaceholder {
    pub id: String,
    pub original_value: String,
    pub placeholder: String,
}

/// Secret substitution engine
pub struct SecretSubstitution {
    detector: SecretDetector,
    secrets: HashMap<String, String>, // placeholder -> original
    counter: usize,
}

impl Default for SecretSubstitution {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretSubstitution {
    pub fn new() -> Self {
        Self {
            detector: SecretDetector::new(),
            secrets: HashMap::new(),
            counter: 0,
        }
    }

    /// Substitute secrets in text with placeholders
    pub fn substitute(&mut self, text: &str) -> String {
        let detected = self.detector.detect(text);
        
        if detected.is_empty() {
            return text.to_string();
        }

        let mut result = String::new();
        let mut last_end = 0;

        for secret in detected {
            // Add text before secret
            result.push_str(&text[last_end..secret.start]);
            
            // Generate placeholder
            self.counter += 1;
            let placeholder = format!("[SECRET_{}]", self.counter);
            
            // Store mapping
            self.secrets.insert(placeholder.clone(), secret.value.clone());
            
            // Add placeholder
            result.push_str(&placeholder);
            
            last_end = secret.end;
        }

        // Add remaining text
        result.push_str(&text[last_end..]);
        
        result
    }

    /// Restore original secrets from placeholders
    pub fn restore(&self, text: &str) -> String {
        let mut result = text.to_string();
        
        for (placeholder, original) in &self.secrets {
            result = result.replace(placeholder, original);
        }
        
        result
    }

    /// Clear all stored secrets
    pub fn clear(&mut self) {
        self.secrets.clear();
        self.counter = 0;
    }

    /// Get number of stored secrets
    pub fn count(&self) -> usize {
        self.secrets.len()
    }

    /// Clone the current secrets map for use in closures
    pub fn clone_secrets(&self) -> HashMap<String, String> {
        self.secrets.clone()
    }

    /// Restore using a pre-cloned secrets map (for use in closures)
    pub fn restore_with(secrets: &HashMap<String, String>, text: &str) -> String {
        let mut result = text.to_string();
        for (placeholder, original) in secrets {
            result = result.replace(placeholder, original);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_and_restore() {
        let mut sub = SecretSubstitution::new();
        
        let original = "API_KEY=sk_test_1234567890abcdef Server at 192.168.1.1";
        let substituted = sub.substitute(original);
        
        // Should contain placeholders
        assert!(substituted.contains("[SECRET_"));
        assert!(!substituted.contains("192.168.1.1"));
        
        // Restore should give back original
        let restored = sub.restore(&substituted);
        assert!(restored.contains("192.168.1.1"));
    }

    #[test]
    fn test_no_secrets() {
        let mut sub = SecretSubstitution::new();
        let text = "Hello world";
        let result = sub.substitute(text);
        assert_eq!(result, text);
    }
}
