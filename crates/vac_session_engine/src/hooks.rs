//! C.5 — hook registry (9 events × 4 command types).
//!
//! Operator-configured hooks under `<project_root>/.vac/hooks.json`.
//! Each hook declares an `event` (PreToolUse, PostToolUse, etc.),
//! a `matcher` (tool-name regex), and a `command` kind (shell /
//! prompt / agent / http). Event names match CC's `HOOK_EVENTS`
//! enum so operators can port hook configs between ecosystems.
//!
//! This module ships the storage primitive + a `HookGate` that
//! wires `PreToolUse` hooks into `CompositeGate` from A.2. The
//! `command` kind runs via `tokio::process::Command`; the other
//! three kinds (`prompt`, `agent`, `http`) return Allow with a
//! log marker for now — real dispatchers land when the tool
//! registry integration in a follow-up crate exposes the
//! adapters these need.
//!
//! Security: shell commands inherit the current process
//! environment + cwd; no shell expansion (argv is split on
//! whitespace, quoted tokens preserved). A hook returning a
//! non-zero exit code maps to `GateDecision::Deny` with the
//! stderr body as the reason.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};
use crate::gate::{GateDecision, ToolCheckCtx, ToolGate};

pub const DEFAULT_HOOKS_FILENAME: &str = "hooks.json";

/// Nine hook events, names aligned with Claude Code's canonical
/// `HOOK_EVENTS` enum for portability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    Stop,
    SubagentStop,
    Notification,
    SessionStart,
    SessionEnd,
    PreCompact,
}

/// Four hook command kinds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookCommand {
    /// Shell-invocable argv. No shell expansion; tokens split on
    /// whitespace with quoted-token preservation at config-parse
    /// time.
    Command { argv: Vec<String> },
    /// Send an LLM completion request with the given system
    /// prompt. Deferred until the LLM adapter is passed in.
    Prompt { prompt: String },
    /// Dispatch a subagent with the given kind + prompt.
    Agent { kind: String, prompt: String },
    /// HTTP POST to URL with JSON body.
    Http { url: String },
}

/// One hook entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookEntry {
    pub id: String,
    pub event: HookEvent,
    /// Regex matched against `ToolCheckCtx.tool_name` (for
    /// PreToolUse / PostToolUse) or left empty for other events.
    #[serde(default)]
    pub matcher: String,
    #[serde(flatten)]
    pub command: HookCommand,
    #[serde(default)]
    pub description: String,
}

/// Persisted collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookStore {
    pub entries: Vec<HookEntry>,
}

/// Soft cap on hook command argv length. Keeps a bogus or
/// malicious hook from spawning a process with hundreds of
/// megabytes of argv bytes that fail during the OS syscall path
/// in ways that are hard to diagnose.
pub const HOOK_ARGV_MAX_LEN: usize = 256;

impl HookStore {
    pub fn resolve_path(project_root: &Path) -> PathBuf {
        project_root.join(".vac").join(DEFAULT_HOOKS_FILENAME)
    }

    /// Create a new entry, rejecting duplicate ids. Symmetric
    /// with [`crate::cron::CronStore::create`] so both stores
    /// surface the same failure shape.
    pub fn create(&mut self, entry: HookEntry) -> EngineResult<()> {
        if self.entries.iter().any(|e| e.id == entry.id) {
            return Err(EngineError::Other(format!(
                "hook id '{}' already registered",
                entry.id
            )));
        }
        if let HookCommand::Command { argv } = &entry.command {
            if argv.len() > HOOK_ARGV_MAX_LEN {
                return Err(EngineError::Other(format!(
                    "hook '{}' argv has {} entries (cap {})",
                    entry.id,
                    argv.len(),
                    HOOK_ARGV_MAX_LEN,
                )));
            }
        }
        self.entries.push(entry);
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    pub async fn load(project_root: &Path) -> EngineResult<Self> {
        let path = Self::resolve_path(project_root);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Self::default());
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(&raw).map_err(EngineError::from)
    }

    pub async fn save(&self, project_root: &Path) -> EngineResult<()> {
        let path = Self::resolve_path(project_root);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(&tmp, raw).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// Select entries matching an event + tool name.
    pub fn matches<'a>(
        &'a self,
        event: HookEvent,
        tool_name: &str,
    ) -> Vec<&'a HookEntry> {
        self.entries
            .iter()
            .filter(|e| e.event == event)
            .filter(|e| {
                if e.matcher.is_empty() {
                    return true;
                }
                match regex::Regex::new(&e.matcher) {
                    Ok(re) => re.is_match(tool_name),
                    Err(_) => false,
                }
            })
            .collect()
    }
}

/// Execute a single hook. For `command` kind this spawns the
/// subprocess; other kinds trace + return Allow until the real
/// adapters land.
pub async fn exec_hook(entry: &HookEntry) -> EngineResult<HookDecision> {
    match &entry.command {
        HookCommand::Command { argv } => {
            if argv.is_empty() {
                return Err(EngineError::Other(format!(
                    "hook '{}' command has empty argv",
                    entry.id,
                )));
            }
            let output = tokio::process::Command::new(&argv[0])
                .args(&argv[1..])
                .output()
                .await?;
            if output.status.success() {
                Ok(HookDecision::Allow)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(HookDecision::Deny {
                    reason: format!(
                        "hook '{}' exited with status {:?}: {}",
                        entry.id,
                        output.status.code(),
                        stderr.trim(),
                    ),
                })
            }
        }
        HookCommand::Prompt { prompt }
        | HookCommand::Agent { prompt, .. } => {
            tracing::warn!(
                target: "vac_tui_runtime::hooks",
                hook = %entry.id,
                kind = if matches!(entry.command, HookCommand::Prompt { .. }) {
                    "prompt"
                } else {
                    "agent"
                },
                prompt = %prompt,
                "hook type not yet dispatched — returning Allow",
            );
            Ok(HookDecision::Allow)
        }
        HookCommand::Http { url } => {
            tracing::warn!(
                target: "vac_tui_runtime::hooks",
                hook = %entry.id,
                url = %url,
                "hook http dispatch deferred — returning Allow",
            );
            Ok(HookDecision::Allow)
        }
    }
}

#[derive(Debug, Clone)]
pub enum HookDecision {
    Allow,
    Deny { reason: String },
}

/// `ToolGate` implementation that fires all PreToolUse hooks
/// matching `ctx.tool_name` and denies if any hook denies.
///
/// Audit fix: the previous implementation recompiled each
/// matcher regex on every call (O(n·compile) per ToolCheckCtx);
/// this version compiles once at `new()` / `replace_store()`
/// time and looks up by id on match.
#[derive(Debug, Clone)]
pub struct HookGate {
    store: std::sync::Arc<tokio::sync::RwLock<HookStoreCompiled>>,
}

/// Internal: HookStore + cached compiled matchers indexed by id.
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
                // None means the matcher was empty (accept all)
                // OR it failed to compile (entry disabled).
                // An empty matcher is recorded as `compiled[id] =
                // None` with `store.matcher = ""`; a bad regex
                // is also `None` but with non-empty matcher text.
                // Keep the "empty = match-all" contract:
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
        // Clone the matching entries so we drop the read lock
        // before each potentially-slow exec_hook call.
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

    fn entry(id: &str, argv: Vec<String>) -> HookEntry {
        HookEntry {
            id: id.into(),
            event: HookEvent::PreToolUse,
            matcher: "Edit|Write".into(),
            command: HookCommand::Command { argv },
            description: String::new(),
        }
    }

    #[test]
    fn matches_picks_entries_by_event_and_regex() {
        let store = HookStore {
            entries: vec![
                entry("h1", vec!["true".into()]),
                HookEntry {
                    id: "h2".into(),
                    event: HookEvent::PostToolUse,
                    ..entry("h2", vec!["true".into()])
                },
            ],
        };
        let pre_edit = store.matches(HookEvent::PreToolUse, "Edit");
        assert_eq!(pre_edit.len(), 1);
        assert_eq!(pre_edit[0].id, "h1");
        let pre_read = store.matches(HookEvent::PreToolUse, "Read");
        assert!(pre_read.is_empty());
    }

    #[tokio::test]
    async fn save_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = HookStore {
            entries: vec![entry("h1", vec!["true".into()])],
        };
        store.save(tmp.path()).await.unwrap();
        let back = HookStore::load(tmp.path()).await.unwrap();
        assert_eq!(back.entries, store.entries);
    }

    #[tokio::test]
    async fn exec_hook_allows_on_zero_exit() {
        let e = entry("ok", vec!["true".into()]);
        let decision = exec_hook(&e).await.unwrap();
        assert!(matches!(decision, HookDecision::Allow));
    }

    #[tokio::test]
    async fn exec_hook_denies_on_nonzero_exit() {
        let e = entry("nope", vec!["false".into()]);
        let decision = exec_hook(&e).await.unwrap();
        match decision {
            HookDecision::Deny { reason } => {
                assert!(reason.contains("hook 'nope'"), "{reason}");
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn create_rejects_duplicate_id() {
        let mut s = HookStore::default();
        s.create(entry("a", vec!["true".into()])).unwrap();
        let err = s
            .create(entry("a", vec!["true".into()]))
            .unwrap_err();
        assert!(format!("{err}").contains("already registered"));
    }

    #[test]
    fn create_rejects_oversized_argv() {
        let mut s = HookStore::default();
        let argv: Vec<String> = (0..HOOK_ARGV_MAX_LEN + 1)
            .map(|i| format!("arg{i}"))
            .collect();
        let err = s.create(entry("big", argv)).unwrap_err();
        assert!(format!("{err}").contains("cap "));
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
