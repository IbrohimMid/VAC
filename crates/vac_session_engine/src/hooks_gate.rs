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
    HookDecision, HookEntry, HookEvent, HookStore, exec_hook,
};

#[derive(Debug, Clone)]
pub struct HookGate {
    store: std::sync::Arc<tokio::sync::RwLock<HookStoreCompiled>>,
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
    pub fn new(store: HookStore) -> Self {
        Self {
            store: std::sync::Arc::new(tokio::sync::RwLock::new(
                HookStoreCompiled::from_store(store),
            )),
        }
    }

    pub async fn replace_store(&self, new: HookStore) {
        *self.store.write().await = HookStoreCompiled::from_store(new);
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
            match exec_hook(entry).await {
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
