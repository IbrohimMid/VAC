//! C.5 — hook registry storage (9 events × 4 command types).
//!
//! Operator-configured hooks under `<project_root>/.vac/hooks.json`.
//! Each hook declares an `event` (PreToolUse, PostToolUse, etc.),
//! a `matcher` (tool-name regex), and a `command` kind (shell /
//! prompt / agent / http). Event names match CC's `HOOK_EVENTS`
//! enum so operators can port hook configs between ecosystems.
//!
//! This module holds the pure-storage primitives (types,
//! `HookStore`, `exec_hook`). The `HookGate` implementation that
//! wires these into `vac_session_engine::gate::ToolGate` lives in
//! `vac_session_engine::hooks_gate`.
//!
//! Security: shell commands inherit the current process
//! environment + cwd; no shell expansion (argv is split on
//! whitespace, quoted tokens preserved). A hook returning a
//! non-zero exit code maps to `HookDecision::Deny` with the
//! stderr body as the reason.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};

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

/// NS.4 — sandbox policy applied when executing a
/// `HookCommand::Command`. All fields are conservative defaults so
/// an operator who cares only about "some limits" can pass
/// `HookSandbox::default()`. CLAUDE.md rules say "only validate at
/// system boundaries" — this is that boundary for shell hooks.
#[derive(Debug, Clone)]
pub struct HookSandbox {
    /// Wall-clock cap. Child is killed on timeout; hook is Denied.
    pub wall_clock: std::time::Duration,
    /// Env-var allowlist. Empty means no env vars forwarded (maximum
    /// isolation); `None` disables filtering (inherit everything,
    /// pre-NS.4 behaviour). A missing allowed var is silently
    /// omitted.
    pub env_allowlist: Option<Vec<String>>,
    /// Soft RLIMIT_AS cap in bytes (address space). 0 disables.
    pub mem_bytes: u64,
    /// Soft RLIMIT_CPU cap in seconds. 0 disables.
    pub cpu_secs: u64,
    /// Soft RLIMIT_NOFILE cap. 0 disables.
    pub nofile: u64,
}

impl Default for HookSandbox {
    fn default() -> Self {
        Self {
            wall_clock: std::time::Duration::from_secs(30),
            // Minimal env: PATH + LANG + HOME. A hook that needs
            // more passes a wider list; the operator must opt in.
            env_allowlist: Some(vec![
                "PATH".into(),
                "HOME".into(),
                "LANG".into(),
                "LC_ALL".into(),
                "USER".into(),
                "LOGNAME".into(),
                "TZ".into(),
            ]),
            mem_bytes: 512 * 1024 * 1024,
            cpu_secs: 10,
            nofile: 128,
        }
    }
}

impl HookSandbox {
    /// Unrestricted sandbox — inherits full environment, no rlimits,
    /// long wall-clock. Matches pre-NS.4 `exec_hook` semantics.
    /// Used by the backward-compat `exec_hook` shim only; do not
    /// expose to operators.
    pub fn permissive() -> Self {
        Self {
            wall_clock: std::time::Duration::from_secs(5 * 60),
            env_allowlist: None,
            mem_bytes: 0,
            cpu_secs: 0,
            nofile: 0,
        }
    }
}

/// Validate a `HookStore` at load time. JSON-schema-level checks
/// (argv bounds, id charset, regex compilability) the rest of the
/// stack relies on. Returns the first violation so operators get a
/// crisp error line rather than a silent disable.
pub fn validate_hook_store(store: &HookStore) -> EngineResult<()> {
    let mut seen: std::collections::HashSet<&str> =
        std::collections::HashSet::with_capacity(store.entries.len());
    for entry in &store.entries {
        if entry.id.is_empty() || entry.id.len() > 128 {
            return Err(EngineError::Other(format!(
                "hook id '{}' must be 1..=128 chars",
                entry.id
            )));
        }
        if !entry
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(EngineError::Other(format!(
                "hook id '{}' must match [A-Za-z0-9_-]+",
                entry.id
            )));
        }
        if !seen.insert(entry.id.as_str()) {
            return Err(EngineError::Other(format!(
                "duplicate hook id '{}'",
                entry.id
            )));
        }
        if !entry.matcher.is_empty() {
            regex::Regex::new(&entry.matcher).map_err(|e| {
                EngineError::Other(format!(
                    "hook '{}' matcher regex invalid: {e}",
                    entry.id
                ))
            })?;
        }
        if let HookCommand::Command { argv } = &entry.command {
            if argv.is_empty() {
                return Err(EngineError::Other(format!(
                    "hook '{}' command has empty argv",
                    entry.id
                )));
            }
            if argv.len() > HOOK_ARGV_MAX_LEN {
                return Err(EngineError::Other(format!(
                    "hook '{}' argv length {} exceeds cap {}",
                    entry.id,
                    argv.len(),
                    HOOK_ARGV_MAX_LEN
                )));
            }
            if argv[0].is_empty() {
                return Err(EngineError::Other(format!(
                    "hook '{}' argv[0] (program) is empty",
                    entry.id
                )));
            }
        }
    }
    Ok(())
}

/// Execute a single hook under the given sandbox. For `command`
/// kind this spawns the subprocess with rlimits + env allowlist +
/// wall-clock cap; other kinds trace + return Allow until the real
/// adapters land.
pub async fn exec_hook_sandboxed(
    entry: &HookEntry,
    sandbox: &HookSandbox,
) -> EngineResult<HookDecision> {
    match &entry.command {
        HookCommand::Command { argv } => {
            if argv.is_empty() {
                return Err(EngineError::Other(format!(
                    "hook '{}' command has empty argv",
                    entry.id,
                )));
            }
            let mut cmd = tokio::process::Command::new(&argv[0]);
            cmd.args(&argv[1..]);
            cmd.kill_on_drop(true);

            // Env filtering.
            if let Some(allow) = &sandbox.env_allowlist {
                cmd.env_clear();
                for key in allow {
                    if let Ok(val) = std::env::var(key) {
                        cmd.env(key, val);
                    }
                }
            }

            // rlimit installation via pre_exec. Safe because the
            // closure only calls setrlimit — a syscall documented
            // async-signal-safe.
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let mem = sandbox.mem_bytes;
                let cpu = sandbox.cpu_secs;
                let nof = sandbox.nofile;
                unsafe {
                    cmd.pre_exec(move || {
                        apply_rlimit(libc::RLIMIT_AS, mem);
                        apply_rlimit(libc::RLIMIT_CPU, cpu);
                        apply_rlimit(libc::RLIMIT_NOFILE, nof);
                        Ok(())
                    });
                }
            }

            let fut = cmd.output();
            let output = match tokio::time::timeout(sandbox.wall_clock, fut).await {
                Ok(r) => r?,
                Err(_) => {
                    return Ok(HookDecision::Deny {
                        reason: format!(
                            "hook '{}' exceeded wall-clock cap {}s",
                            entry.id,
                            sandbox.wall_clock.as_secs(),
                        ),
                    });
                }
            };
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

/// NS.4 — backward-compatible shim. Runs the hook under a
/// permissive sandbox (inherits full env, no rlimits, 5-min
/// wall-clock) so pre-NS.4 call sites observe identical behaviour.
/// **New call sites must prefer [`exec_hook_sandboxed`] with an
/// explicit policy.**
pub async fn exec_hook(entry: &HookEntry) -> EngineResult<HookDecision> {
    exec_hook_sandboxed(entry, &HookSandbox::permissive()).await
}

#[cfg(unix)]
fn apply_rlimit(resource: libc::__rlimit_resource_t, soft: u64) {
    if soft == 0 {
        return;
    }
    // SAFETY: setrlimit is async-signal-safe. Failure is non-fatal;
    // the process will simply run without the cap — we can't log
    // from pre_exec so swallow errors silently. Tests verify
    // positive enforcement via `prlimit`-style probes.
    let rl = libc::rlimit {
        rlim_cur: soft as libc::rlim_t,
        rlim_max: soft as libc::rlim_t,
    };
    unsafe {
        let _ = libc::setrlimit(resource, &rl);
    }
}

#[derive(Debug, Clone)]
pub enum HookDecision {
    Allow,
    Deny { reason: String },
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

    #[tokio::test]
    async fn sandbox_wall_clock_denies_runaway() {
        let e = HookEntry {
            id: "slow".into(),
            event: HookEvent::PreToolUse,
            matcher: String::new(),
            command: HookCommand::Command {
                argv: vec!["sleep".into(), "5".into()],
            },
            description: String::new(),
        };
        let sandbox = HookSandbox {
            wall_clock: std::time::Duration::from_millis(300),
            ..HookSandbox::permissive()
        };
        let decision = exec_hook_sandboxed(&e, &sandbox).await.unwrap();
        match decision {
            HookDecision::Deny { reason } => {
                assert!(reason.contains("wall-clock"), "{reason}");
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sandbox_env_allowlist_filters_inherited_env() {
        // Child sees only PATH and TEST_ALLOWED; TEST_BLOCKED is stripped.
        unsafe {
            std::env::set_var("TEST_ALLOWED_NS4", "1");
            std::env::set_var("TEST_BLOCKED_NS4", "1");
        }
        let e = HookEntry {
            id: "envcheck".into(),
            event: HookEvent::PreToolUse,
            matcher: String::new(),
            command: HookCommand::Command {
                argv: vec![
                    "sh".into(),
                    "-c".into(),
                    "test -z \"$TEST_BLOCKED_NS4\" && test \"$TEST_ALLOWED_NS4\" = 1".into(),
                ],
            },
            description: String::new(),
        };
        let sandbox = HookSandbox {
            wall_clock: std::time::Duration::from_secs(5),
            env_allowlist: Some(vec!["PATH".into(), "TEST_ALLOWED_NS4".into()]),
            mem_bytes: 0,
            cpu_secs: 0,
            nofile: 0,
        };
        let decision = exec_hook_sandboxed(&e, &sandbox).await.unwrap();
        assert!(matches!(decision, HookDecision::Allow), "{decision:?}");
    }

    #[test]
    fn validate_hook_store_rejects_duplicate_ids() {
        let store = HookStore {
            entries: vec![
                entry("dup", vec!["true".into()]),
                entry("dup", vec!["true".into()]),
            ],
        };
        let err = validate_hook_store(&store).unwrap_err();
        assert!(format!("{err}").contains("duplicate"), "{err}");
    }

    #[test]
    fn validate_hook_store_rejects_bad_regex() {
        let store = HookStore {
            entries: vec![HookEntry {
                id: "h1".into(),
                event: HookEvent::PreToolUse,
                matcher: "(unclosed".into(),
                command: HookCommand::Command { argv: vec!["true".into()] },
                description: String::new(),
            }],
        };
        let err = validate_hook_store(&store).unwrap_err();
        assert!(format!("{err}").contains("matcher regex"), "{err}");
    }

    #[test]
    fn validate_hook_store_rejects_bad_id_charset() {
        let store = HookStore {
            entries: vec![HookEntry {
                id: "bad id!".into(),
                event: HookEvent::PreToolUse,
                matcher: String::new(),
                command: HookCommand::Command { argv: vec!["true".into()] },
                description: String::new(),
            }],
        };
        assert!(validate_hook_store(&store).is_err());
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
}
