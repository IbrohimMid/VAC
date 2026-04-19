//! Hierarchical shell approval scopes for `bash` tool.
//!
//! Parses a shell command into a scope path like:
//!   `bash::git::status`
//!   `bash::cargo::check`
//!   `bash::rm::-rf`
//!
//! Policy is checked from most-specific to least-specific scope.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A tool call that was intercepted by the approval gate and not yet resolved.
/// Serialized into checkpoint metadata so it can be re-surfaced on resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub tool_call_id: String,
    pub tool_name: String,
    pub scope: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopePolicy {
    Allow,
    Deny,
    Prompt,
}

/// Hierarchical approval policy for shell commands.
#[derive(Debug, Clone, Default)]
pub struct ShellApprovalPolicy {
    /// Map from scope string (e.g. "bash::rm::-rf") to policy.
    rules: HashMap<String, ScopePolicy>,
}

impl ShellApprovalPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from TOML [approvals] section.
    pub fn from_toml(table: &toml::Table) -> Self {
        let mut policy = Self::new();
        let approvals = match table.get("approvals").and_then(|v| v.as_table()) {
            Some(t) => t,
            None => return policy,
        };

        for (decision_str, value) in approvals {
            let decision = match decision_str.as_str() {
                "allow" => ScopePolicy::Allow,
                "deny" => ScopePolicy::Deny,
                "prompt" => ScopePolicy::Prompt,
                _ => continue,
            };
            if let Some(scopes) = value.as_array() {
                for scope in scopes.iter().filter_map(|v| v.as_str()) {
                    policy.rules.insert(scope.to_string(), decision.clone());
                }
            }
        }

        policy
    }

    /// Evaluate a shell command against the policy.
    /// Returns the most-specific matching rule, or None if no rule matches.
    pub fn evaluate(&self, command: &str) -> Option<&ScopePolicy> {
        let scopes = parse_command_scopes(command);
        // Check from most-specific to least-specific
        for scope in scopes.iter().rev() {
            if let Some(policy) = self.rules.get(scope) {
                return Some(policy);
            }
        }
        None
    }

    /// Check if a command is hard-denied.
    pub fn is_denied(&self, command: &str) -> Option<String> {
        if let Some(ScopePolicy::Deny) = self.evaluate(command) {
            let scopes = parse_command_scopes(command);
            let matched = scopes
                .iter()
                .rev()
                .find(|s| self.rules.get(*s) == Some(&ScopePolicy::Deny));
            return Some(format!(
                "denied by scope `{}`",
                matched.cloned().unwrap_or_else(|| "bash".to_string())
            ));
        }
        None
    }
}

/// Parse a shell command into a list of scope strings from least to most specific.
///
/// Examples:
///   "git status" → ["bash", "bash::git", "bash::git::status"]
///   "rm -rf /" → ["bash", "bash::rm", "bash::rm::-rf"]
///   "cargo check --release" → ["bash", "bash::cargo", "bash::cargo::check"]
pub fn parse_command_scopes(command: &str) -> Vec<String> {
    let tokens = tokenize(command);
    let mut scopes = vec!["bash".to_string()];

    if let Some(program) = tokens.first() {
        let prog = program.as_str();
        scopes.push(format!("bash::{prog}"));

        if let Some(subcommand) = tokens.get(1) {
            // Skip flags for subcommand detection
            if !subcommand.starts_with('-') {
                scopes.push(format!("bash::{prog}::{subcommand}"));
            } else {
                // First arg is a flag — include it as the scope
                scopes.push(format!("bash::{prog}::{subcommand}"));
            }
        }
    }

    scopes
}

/// Minimal shell tokenizer — splits on whitespace, handles basic quoting.
fn tokenize(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in command.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Default safe policy — used when no config is present.
pub fn default_policy() -> ShellApprovalPolicy {
    let mut p = ShellApprovalPolicy::new();
    // Hard denies
    for scope in &[
        "bash::rm::-rf",
        "bash::rm::--recursive",
        "bash::kubectl::delete",
        "bash::kubectl::drain",
        "bash::dd",
        "bash::mkfs",
        "bash::shutdown",
        "bash::reboot",
    ] {
        p.rules.insert(scope.to_string(), ScopePolicy::Deny);
    }
    // Explicit allows for common dev tasks
    for scope in &[
        "bash::git::status",
        "bash::git::log",
        "bash::git::diff",
        "bash::git::add",
        "bash::git::commit",
        "bash::git::push",
        "bash::git::pull",
        "bash::cargo::check",
        "bash::cargo::build",
        "bash::cargo::test",
        "bash::cargo::clippy",
        "bash::cargo::fmt",
        "bash::ls",
        "bash::cat",
        "bash::echo",
        "bash::printf",
        "bash::pwd",
        "bash::which",
        "bash::grep",
        "bash::find",
        "bash::mkdir",
        "bash::cp",
        "bash::mv",
    ] {
        p.rules.insert(scope.to_string(), ScopePolicy::Allow);
    }
    p
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_parsing() {
        assert_eq!(
            parse_command_scopes("git status"),
            vec!["bash", "bash::git", "bash::git::status"]
        );
        assert_eq!(
            parse_command_scopes("rm -rf /"),
            vec!["bash", "bash::rm", "bash::rm::-rf"]
        );
        assert_eq!(
            parse_command_scopes("cargo check"),
            vec!["bash", "bash::cargo", "bash::cargo::check"]
        );
    }

    #[test]
    fn test_default_policy_denies_rm_rf() {
        let policy = default_policy();
        assert!(policy.is_denied("rm -rf /").is_some());
        assert!(policy.is_denied("rm -rf .").is_some());
    }

    #[test]
    fn test_default_policy_allows_git_status() {
        let policy = default_policy();
        assert_eq!(policy.evaluate("git status"), Some(&ScopePolicy::Allow));
    }
}
