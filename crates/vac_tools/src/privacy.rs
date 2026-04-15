//! Privacy vault — secret substitution for LLM privacy.
//!
//! Allows AI to work with secrets (API keys, tokens, IPs) without
//! the LLM ever seeing raw values. Values are substituted with
//! aliases like SECRET_API_KEY_1, then restored for tool execution.

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

struct Patterns {
    aws_key: Regex,
    api_key: Regex,
    bearer: Regex,
    ip: Regex,
    aws_account: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        aws_key: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
        api_key: Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(),
        bearer: Regex::new(r"(?i)Bearer\s+([A-Za-z0-9\-._~+/]+=*)").unwrap(),
        ip: Regex::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
        )
        .unwrap(),
        aws_account: Regex::new(r"\b\d{12}\b").unwrap(),
    })
}

#[derive(Default)]
pub struct PrivacyVault {
    map: HashMap<String, String>,
    reverse: HashMap<String, String>,
    counters: HashMap<&'static str, usize>,
}

impl PrivacyVault {
    pub fn new() -> Self {
        Self::default()
    }

    fn alias(&mut self, category: &'static str, secret: &str) -> String {
        if let Some(a) = self.reverse.get(secret) {
            return a.clone();
        }
        let n = self.counters.entry(category).or_insert(0);
        *n += 1;
        let alias = format!("SECRET_{}_{}", category, n);
        self.map.insert(alias.clone(), secret.to_string());
        self.reverse.insert(secret.to_string(), alias.clone());
        alias
    }

    pub fn substitute(&mut self, text: &str) -> String {
        let p = patterns();
        let mut out = text.to_string();
        out = p
            .aws_key
            .replace_all(&out, |caps: &regex::Captures| {
                self.alias("AWS_KEY", &caps[0])
            })
            .into_owned();
        out = p
            .api_key
            .replace_all(&out, |caps: &regex::Captures| {
                self.alias("API_KEY", &caps[0])
            })
            .into_owned();
        out = p
            .bearer
            .replace_all(&out, |caps: &regex::Captures| {
                let alias = self.alias("BEARER", &caps[1]);
                format!("Bearer {}", alias)
            })
            .into_owned();
        out = p
            .aws_account
            .replace_all(&out, |caps: &regex::Captures| {
                self.alias("AWS_ACCOUNT_ID", &caps[0])
            })
            .into_owned();
        out =
            p.ip.replace_all(&out, |caps: &regex::Captures| self.alias("IP", &caps[0]))
                .into_owned();
        out
    }

    pub fn restore(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (alias, secret) in &self.map {
            out = out.replace(alias.as_str(), secret.as_str());
        }
        out
    }

    pub fn substitute_value(&mut self, v: Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.substitute(&s)),
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|x| self.substitute_value(x)).collect())
            }
            Value::Object(obj) => Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, self.substitute_value(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    pub fn restore_value(&self, v: Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.restore(&s)),
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|x| self.restore_value(x)).collect())
            }
            Value::Object(obj) => Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, self.restore_value(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    pub fn aliases(&self) -> Vec<(String, String)> {
        self.map
            .iter()
            .map(|(alias, secret)| {
                let preview = if secret.len() > 4 {
                    format!("{}****", &secret[..4])
                } else {
                    "****".to_string()
                };
                (alias.clone(), preview)
            })
            .collect()
    }

    pub fn clone_secrets(&self) -> HashMap<String, String> {
        self.map.clone()
    }

    pub fn restore_with(secrets: &HashMap<String, String>, text: &str) -> String {
        let mut out = text.to_string();
        for (alias, secret) in secrets {
            out = out.replace(alias.as_str(), secret.as_str());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_aws_key() {
        let mut vault = PrivacyVault::new();
        let text = "Key: AKIAIOSFODNN7EXAMPLE";
        let sub = vault.substitute(text);
        assert!(sub.contains("SECRET_AWS_KEY_1"));
        assert!(!sub.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn restore_substituted() {
        let mut vault = PrivacyVault::new();
        let original = "Key: AKIAIOSFODNN7EXAMPLE";
        let sub = vault.substitute(original);
        let restored = vault.restore(&sub);
        assert_eq!(restored, original);
    }

    #[test]
    fn substitute_json_value() {
        let mut vault = PrivacyVault::new();
        let json = serde_json::json!({
            "api_key": "sk-test12345678901234567890",
            "nested": {
                "ip": "192.168.1.1"
            }
        });
        let sub = vault.substitute_value(json);
        let sub_str = serde_json::to_string(&sub).unwrap();
        assert!(sub_str.contains("SECRET_API_KEY_1"));
        assert!(sub_str.contains("SECRET_IP_1"));
    }
}
