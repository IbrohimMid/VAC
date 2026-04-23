//! W4.2 — per-MCP-server channel ACL.
//!
//! An MCP server can expose several logical channels (tools, resources,
//! prompts, sampling, logging). Some servers are fine with full access;
//! others should be restricted to read-only queries. Channel ACL lets
//! the operator write:
//!
//! ```toml
//! [mcp.servers.github]
//! allow_channels = ["tools", "resources"]
//! deny_channels  = ["sampling"]
//! notify_channels = ["logging"]
//! ```
//!
//! Policy:
//!
//! 1. If the channel is in `deny`, deny — wins over everything.
//! 2. If `allow` is non-empty and the channel is not in it, deny.
//! 3. Otherwise allow. `notify` is a logging-only hint, no effect
//!    on the verdict.
//!
//! Applied **after** the trust-class check in
//! `vac_tools::trust_gate::TrustGate::check_mcp_tool`. The trust class
//! says "is this server allowed to be called at all?"; the channel
//! ACL refines "and is this particular capability of the server
//! allowed on this call?".

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Per-server ACL. Default = allow everything, notify nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelAcl {
    #[serde(default)]
    pub allow_channels: Vec<String>,
    #[serde(default)]
    pub deny_channels: Vec<String>,
    #[serde(default)]
    pub notify_channels: Vec<String>,
}

/// Verdict the caller applies on top of the trust-class verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelDecision {
    /// Channel allowed; proceed.
    Allow,
    /// Channel allowed but operator wants a log entry.
    AllowWithNotify(String),
    /// Channel denied; the string identifies the rule (`deny` or
    /// `not-in-allow`) so the audit log can cite it.
    Deny(String),
}

impl ChannelAcl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge two ACLs. The second wins on collisions; used when a
    /// project-scope ACL overrides a user-scope default.
    pub fn merge(base: &Self, overlay: &Self) -> Self {
        let mut merged = base.clone();
        if !overlay.allow_channels.is_empty() {
            merged.allow_channels = overlay.allow_channels.clone();
        }
        // Deny is the union — stricter wins.
        let denies: HashSet<String> = merged
            .deny_channels
            .iter()
            .chain(overlay.deny_channels.iter())
            .cloned()
            .collect();
        merged.deny_channels = denies.into_iter().collect();
        // Notify is union too — notify hints compose.
        let notifies: HashSet<String> = merged
            .notify_channels
            .iter()
            .chain(overlay.notify_channels.iter())
            .cloned()
            .collect();
        merged.notify_channels = notifies.into_iter().collect();
        merged
    }

    /// Decide for a concrete channel name. Channel strings should be
    /// lowercase; the ACL does case-sensitive match so the caller is
    /// responsible for normalisation.
    pub fn check(&self, channel: &str) -> ChannelDecision {
        if self.deny_channels.iter().any(|c| c == channel) {
            return ChannelDecision::Deny(format!(
                "channel {channel} in deny list"
            ));
        }
        if !self.allow_channels.is_empty()
            && !self.allow_channels.iter().any(|c| c == channel)
        {
            return ChannelDecision::Deny(format!(
                "channel {channel} not in allow list"
            ));
        }
        if self.notify_channels.iter().any(|c| c == channel) {
            return ChannelDecision::AllowWithNotify(format!(
                "channel {channel} on notify list"
            ));
        }
        ChannelDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acl(allow: &[&str], deny: &[&str], notify: &[&str]) -> ChannelAcl {
        ChannelAcl {
            allow_channels: allow.iter().map(|s| s.to_string()).collect(),
            deny_channels: deny.iter().map(|s| s.to_string()).collect(),
            notify_channels: notify.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn default_allows_everything() {
        let a = ChannelAcl::default();
        assert_eq!(a.check("tools"), ChannelDecision::Allow);
        assert_eq!(a.check("sampling"), ChannelDecision::Allow);
    }

    #[test]
    fn deny_always_wins() {
        let a = acl(&["tools", "sampling"], &["sampling"], &[]);
        match a.check("sampling") {
            ChannelDecision::Deny(r) => assert!(r.contains("deny list")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn allow_list_excludes_unlisted_channels() {
        let a = acl(&["tools"], &[], &[]);
        match a.check("resources") {
            ChannelDecision::Deny(r) => assert!(r.contains("allow list")),
            other => panic!("expected Deny, got {other:?}"),
        }
        assert_eq!(a.check("tools"), ChannelDecision::Allow);
    }

    #[test]
    fn notify_emits_hint_not_deny() {
        let a = acl(&[], &[], &["logging"]);
        match a.check("logging") {
            ChannelDecision::AllowWithNotify(r) => {
                assert!(r.contains("notify list"));
            }
            other => panic!("expected AllowWithNotify, got {other:?}"),
        }
    }

    #[test]
    fn merge_project_overrides_user_allow() {
        let user = acl(&["tools"], &[], &[]);
        let project = acl(&["resources"], &[], &[]);
        let m = ChannelAcl::merge(&user, &project);
        // Project's allow list wins wholesale.
        assert_eq!(
            m.allow_channels.iter().collect::<std::collections::HashSet<_>>(),
            ["resources".to_string()].iter().collect::<std::collections::HashSet<_>>(),
        );
    }

    #[test]
    fn merge_deny_is_union() {
        let a = acl(&[], &["x"], &[]);
        let b = acl(&[], &["y"], &[]);
        let m = ChannelAcl::merge(&a, &b);
        let set: std::collections::HashSet<_> = m.deny_channels.iter().collect();
        assert!(set.contains(&"x".to_string()));
        assert!(set.contains(&"y".to_string()));
    }

    #[test]
    fn serde_roundtrip() {
        let a = acl(&["tools"], &["sampling"], &["logging"]);
        let s = serde_json::to_string(&a).unwrap();
        let back: ChannelAcl = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }
}
