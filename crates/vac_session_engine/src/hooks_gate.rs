//! `HookGate` — wires `PreToolUse` hooks into the session-engine
//! `ToolGate` contract. Storage primitives live in
//! [`vac_session_primitives::hooks`]; this file holds the
//! session-runtime-aware `ToolGate` impl that depends on the
//! engine's `gate` module (which in turn would pull the primitives
//! crate into the wrong direction if inlined there).
//!
//! Audit note: matchers compile once at `new()` / `replace_store()`
//! time rather than per-call.

use async_trait::async_trait;

use crate::gate::{GateDecision, ToolCheckCtx, ToolGate};
use vac_session_primitives::hooks::{
    HookDecision, HookEntry, HookEvent, HookSandbox, HookStore, exec_hook_sandboxed,
};

#[derive(Debug, Clone)]
pub struct HookGate {
    store: std::sync::Arc<tokio::sync::RwLock<HookStoreCompiled>>,
    sandbox: std::sync::Arc<HookSandbox>,
}

#[derive(Debug)]
pub(crate) struct HookStoreCompiled {
    pub store: HookStore,
    pub compiled: std::collections::HashMap<String, Option<regex::Regex>>,
}

impl HookStoreCompiled {
    fn from_store(store: HookStore) -> Self {
        let mut compiled = std::collections::HashMap::with_capacity(store.entries.len());
        for e in &store.entries {
            let r = if e.matcher.is_empty() {
                None
            } else {
                match regex::Regex::new(&e.matcher) {
                    Ok(r) => Some(r),
                    Err(err) => {
                        tracing::warn!(
                            target: "vac_tui_runtime::hooks",
                            hook = %e.id,
                            matcher = %e.matcher,
                            error = %err,
                            "hook matcher failed to compile — entry disabled",
                        );
                        None
                    }
                }
            };
            compiled.insert(e.id.clone(), r);
        }
        Self { store, compiled }
    }

    fn matches<'a>(&'a self, event: HookEvent, tool_name: &str) -> Vec<&'a HookEntry> {
        self.store
            .entries
            .iter()
            .filter(|e| e.event == event)
            .filter(|e| match self.compiled.get(&e.id) {
                Some(Some(re)) => re.is_match(tool_name),
                Some(None) => e.matcher.is_empty(),
                None => false,
            })
            .collect()
    }
}

impl HookGate {
    /// Default gate — applies `HookSandbox::default()` (restrictive)
    /// to every shell hook. Matches the NS.4 posture: sandbox-on
    /// unless the operator opts into a wider policy.
    pub fn new(store: HookStore) -> Self {
        Self::with_sandbox(store, HookSandbox::default())
    }

    pub fn with_sandbox(store: HookStore, sandbox: HookSandbox) -> Self {
        Self {
            store: std::sync::Arc::new(tokio::sync::RwLock::new(HookStoreCompiled::from_store(
                store,
            ))),
            sandbox: std::sync::Arc::new(sandbox),
        }
    }

    pub async fn replace_store(&self, new: HookStore) {
        *self.store.write().await = HookStoreCompiled::from_store(new);
    }

    /// C12: Async, timeout-bounded, and redacted execution for notification hooks.
    /// Fires `event` matching hooks (with empty tool_name match) in the background.
    pub fn fire_notification(&self, event: HookEvent, payload: Option<String>) {
        let store = self.store.clone();
        let mut sandbox = (*self.sandbox).clone();
        
        // Ensure env variables are allowed for this execution
        if let Some(ref mut allowlist) = sandbox.env_allowlist {
            allowlist.push("VAC_HOOK_EVENT".to_string());
            allowlist.push("VAC_HOOK_PAYLOAD".to_string());
        }

        tokio::spawn(async move {
            let store_guard = store.read().await;
            // Notification hooks usually have empty matchers, but we pass an empty string
            // so `matches` filters by event type.
            let entries: Vec<HookEntry> = store_guard.matches(event, "").into_iter().cloned().collect();
            drop(store_guard);

            for entry in entries {
                let mut extra_env = std::collections::HashMap::new();
                extra_env.insert("VAC_HOOK_EVENT".to_string(), format!("{:?}", event));
                if let Some(ref p) = payload {
                    extra_env.insert("VAC_HOOK_PAYLOAD".to_string(), p.clone());
                }

                // Fire and forget; timeout is handled by the sandbox.
                let _ = exec_hook_sandboxed(&entry, &sandbox, Some(extra_env)).await;
            }
        });
    }
}

#[async_trait]
impl ToolGate for HookGate {
    fn label(&self) -> &'static str {
        "hooks"
    }

    async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision {
        let store = self.store.read().await;
        let matches = store.matches(HookEvent::PreToolUse, &ctx.tool_name);
        let matches: Vec<HookEntry> = matches.iter().copied().cloned().collect();
        drop(store);
        for entry in &matches {
            match exec_hook_sandboxed(entry, &self.sandbox, None).await {
                Ok(HookDecision::Allow) => continue,
                Ok(HookDecision::Deny { reason }) => {
                    return GateDecision::Deny { reason };
                }
                Err(e) => {
                    tracing::warn!(
                        target: "vac_tui_runtime::hooks",
                        hook = %entry.id,
                        error = %e,
                        "hook execution failed — treating as deny",
                    );
                    return GateDecision::Deny {
                        reason: format!("hook '{}' failed: {e}", entry.id),
                    };
                }
            }
        }
        GateDecision::allow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_session_primitives::hooks::HookCommand;

    fn entry(id: &str, argv: Vec<String>) -> HookEntry {
        HookEntry {
            id: id.into(),
            event: HookEvent::PreToolUse,
            matcher: "Edit|Write".into(),
            command: HookCommand::Command { argv },
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn hook_gate_denies_when_hook_denies() {
        use uuid::Uuid;
        let gate = HookGate::new(HookStore {
            entries: vec![entry("nope", vec!["false".into()])],
        });
        let ctx = ToolCheckCtx::new("Edit", Uuid::new_v4());
        match gate.check(&ctx).await {
            GateDecision::Deny { reason } => {
                assert!(reason.contains("hook 'nope'"));
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }
}
