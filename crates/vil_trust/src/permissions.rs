//! Permission types and sets.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    FileRead,
    FileWrite,
    FileDelete,
    ShellExec,
    GitRead,
    GitWrite,
    CargoRun,
    NetworkAccess,
    MemoryRead,
    MemoryWrite,
    McpInvoke,
    Custom(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    granted: HashSet<Permission>,
    denied: HashSet<Permission>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, perm: Permission) {
        self.denied.remove(&perm);
        self.granted.insert(perm);
    }

    pub fn deny(&mut self, perm: Permission) {
        self.granted.remove(&perm);
        self.denied.insert(perm);
    }

    pub fn is_granted(&self, perm: &Permission) -> bool {
        self.granted.contains(perm) && !self.denied.contains(perm)
    }

    pub fn is_denied(&self, perm: &Permission) -> bool {
        self.denied.contains(perm)
    }
}
