//! Permission types and sets.
//!
//! Permissions are the fine-grained counterpart to [`crate::TrustZone`]. A
//! [`PermissionSet`] explicitly grants or denies individual [`Permission`]s,
//! letting callers express policies like "untrusted + allow GitRead".

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A single permission an agent may be granted or denied.
///
/// Denies always win over grants in [`PermissionSet::is_granted`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read files inside the project root.
    FileRead,
    /// Create or modify files inside the project root.
    FileWrite,
    /// Delete files.
    FileDelete,
    /// Invoke shell commands.
    ShellExec,
    /// Read-only git operations (status, log, diff).
    GitRead,
    /// Mutating git operations (commit, push, branch).
    GitWrite,
    /// Run cargo / other build-tool commands.
    CargoRun,
    /// Make outbound network requests.
    NetworkAccess,
    /// Read from the agent memory store.
    MemoryRead,
    /// Write to the agent memory store.
    MemoryWrite,
    /// Invoke tools exposed over the Model Context Protocol.
    McpInvoke,
    /// Project-specific permission with a user-defined identifier.
    Custom(String),
}

/// A positive/negative set of permissions. Denies take precedence over grants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    granted: HashSet<Permission>,
    denied: HashSet<Permission>,
}

impl PermissionSet {
    /// Creates an empty set with no grants or denies.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grants the permission and removes any prior deny for it.
    pub fn grant(&mut self, perm: Permission) {
        self.denied.remove(&perm);
        self.granted.insert(perm);
    }

    /// Denies the permission and removes any prior grant for it.
    pub fn deny(&mut self, perm: Permission) {
        self.granted.remove(&perm);
        self.denied.insert(perm);
    }

    /// Returns `true` iff the permission is granted and not denied.
    pub fn is_granted(&self, perm: &Permission) -> bool {
        self.granted.contains(perm) && !self.denied.contains(perm)
    }

    /// Returns `true` iff the permission is explicitly denied.
    pub fn is_denied(&self, perm: &Permission) -> bool {
        self.denied.contains(perm)
    }
}
