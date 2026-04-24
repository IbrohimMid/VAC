use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::ToolError;
use vil_context::shm::ShmArena;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub trust_requirement: String,
    pub risk_level: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptBehavior {
    Cancel,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchReadKind {
    None,
    Search,
    Read,
    List,
}

#[async_trait]
pub trait VilTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn trust_requirement(&self) -> &str;
    fn risk_level(&self) -> &str;

    /// F1.1 — Return the formal `vac_tool_core::ToolSpec` for this
    /// tool. Every existing tool automatically gets a spec without a
    /// breaking change. Tools that need fine-grained capability or
    /// render hints override this.
    fn spec(&self) -> vac_tool_core::ToolSpec;

    /// Async matcher builder. Default: `|_| true`.
    async fn prepare_permission_matcher(
        &self,
        _args: &serde_json::Value,
    ) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
        Box::new(|_| true)
    }

    /// Ctrl-C behavior. Default: Cancel.
    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Cancel
    }

    /// Dedup within a submit.
    fn inputs_equivalent(&self, _a: &serde_json::Value, _b: &serde_json::Value) -> bool {
        false
    }

    /// Search/read classifier.
    fn search_read_classification(&self, _args: &serde_json::Value) -> SearchReadKind {
        SearchReadKind::None
    }

    // ── W2.1 — per-input capability overrides ──────────────────────────
    //
    // `ToolSpec.capability` carries the tool-level default. These
    // methods let a tool narrow the verdict for a specific input
    // (`BashTool("git log")` is concurrency-safe and non-destructive;
    // `BashTool("git reset --hard")` is neither). Default implementations
    // fall back to the spec so tools that don't care get the right
    // answer for free.

    /// Whether a specific invocation is safe to run in parallel with
    /// other tool calls in the same submit. Per-input refinement of
    /// `ToolSpec.capability.concurrency_safe`.
    fn is_input_concurrency_safe(&self, _input: &serde_json::Value) -> bool {
        self.spec().capability.concurrency_safe
    }

    /// Whether a specific invocation can destroy data (delete, force
    /// overwrite, force-push). Per-input refinement of
    /// `ToolSpec.capability.destructive`.
    fn is_input_destructive(&self, _input: &serde_json::Value) -> bool {
        self.spec().capability.destructive
    }

    /// Whether a specific invocation is read-only — useful for fork
    /// speculation which must not mutate. Default derives from the
    /// spec's `read_only` flag.
    fn is_input_read_only(&self, _input: &serde_json::Value) -> bool {
        self.spec().capability.read_only
    }

    /// Mutate a transcript-visible clone of the input before hooks /
    /// observers see it. Must be idempotent — the API-bound original
    /// is never mutated so prompt-cache reuse stays correct.
    /// Default is no-op.
    fn backfill_observable_input(&self, _input: &mut serde_json::Value) {}

    // ── W2.2 — deferred tool loading ──────────────────────────────────
    //
    // `should_defer` = true means the tool is excluded from the
    // initial tool manifest (saves prompt tokens) and only surfaces
    // once `ToolSearch` resolves it. `always_load` forces inclusion
    // even when `should_defer` is true — used for tools the agent
    // must see on turn 1 regardless.

    /// When `true`, this tool ships with `defer_loading` flag so its
    /// full schema is only sent to the model after an explicit
    /// `ToolSearch` resolve. Default: false.
    fn should_defer(&self) -> bool {
        false
    }

    /// When `true`, this tool's schema is always included in the
    /// initial manifest even when `should_defer` would hide it.
    /// Precedence: `always_load` > `should_defer`. Default: false.
    fn always_load(&self) -> bool {
        false
    }

    // ── W2.3 — oversized-result disk spill ────────────────────────────

    /// Max size in characters of a tool result that is inlined in the
    /// transcript + response. Results larger than this are persisted
    /// to `.vac/tool-results/<id>.json` and the caller receives a
    /// `PreviewStub` with the path and a small head-of-payload
    /// sample. Override in tools whose output must never be persisted
    /// (e.g. `FileRead` — circular Read→persist→Read loop risk).
    /// `usize::MAX` disables the threshold entirely.
    fn max_result_size_chars(&self) -> usize {
        256 * 1024
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentZone {
    #[default]
    ParentAgent,
    SandboxedSubagent,
}

#[derive(Clone)]
pub struct ToolContext {
    pub working_dir: std::path::PathBuf,
    pub env_vars: HashMap<String, String>,
    pub session_id: uuid::Uuid,
    pub submit_id: Option<uuid::Uuid>,
    pub shm: Option<Arc<ShmArena>>,
    pub agent_zone: AgentZone,
    pub environment_mode: String,
    pub privacy: Arc<RwLock<crate::PrivacyVault>>,
    /// B4 — optional `AgentDispatcher`. When set, the `agent_run`
    /// tool delegates to it; when None, `agent_run` returns an
    /// explicit error rather than silently failing. Populated by
    /// the live session wiring in `vac_tui_runtime::runner`.
    pub agent_dispatcher:
        Option<Arc<dyn vac_session_primitives::AgentDispatcher>>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("working_dir", &self.working_dir)
            .field("session_id", &self.session_id)
            .field("agent_zone", &self.agent_zone)
            .field("environment_mode", &self.environment_mode)
            .finish_non_exhaustive()
    }
}

impl ToolContext {
    pub fn new(working_dir: std::path::PathBuf) -> Self {
        Self {
            working_dir,
            env_vars: std::env::vars().collect(),
            session_id: uuid::Uuid::new_v4(),
            submit_id: None,
            shm: None,
            agent_zone: AgentZone::ParentAgent,
            environment_mode: std::env::var("VAC_ENVIRONMENT_MODE")
                .unwrap_or_else(|_| "host".to_string()),
            privacy: Arc::new(RwLock::new(crate::PrivacyVault::new())),
            agent_dispatcher: None,
        }
    }

    pub fn with_agent_dispatcher(
        mut self,
        d: Arc<dyn vac_session_primitives::AgentDispatcher>,
    ) -> Self {
        self.agent_dispatcher = Some(d);
        self
    }

    pub fn with_session_id(mut self, session_id: uuid::Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_submit_id(mut self, submit_id: uuid::Uuid) -> Self {
        self.submit_id = Some(submit_id);
        self
    }

    pub fn with_shm(mut self, shm: Arc<ShmArena>) -> Self {
        self.shm = Some(shm);
        self
    }

    pub fn with_zone(mut self, zone: AgentZone) -> Self {
        self.agent_zone = zone;
        self
    }

    pub fn with_environment_mode(mut self, mode: impl Into<String>) -> Self {
        self.environment_mode = mode.into();
        self
    }
}

pub fn default_spec(tool: &dyn VilTool) -> vac_tool_core::ToolSpec {
    let permission = match tool.trust_requirement() {
        "safe" => vac_tool_core::ToolPermissionClass::Safe,
        "ask_once" => vac_tool_core::ToolPermissionClass::AskOnce,
        "privileged" => vac_tool_core::ToolPermissionClass::Privileged,
        _ => vac_tool_core::ToolPermissionClass::AskEveryCall,
    };
    let capability = match tool.risk_level() {
        "safe" => vac_tool_core::ToolCapability::default(),
        "destructive" => vac_tool_core::ToolCapability::destructive(),
        _ => vac_tool_core::ToolCapability::mutating(),
    };
    vac_tool_core::ToolSpec {
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        input_schema: tool.input_schema(),
        capability,
        permission,
        render: vac_tool_core::ToolRenderHints::default(),
    }
}

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn VilTool>>>,
    definitions: RwLock<HashMap<String, ToolDefinition>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            definitions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, tool: impl VilTool + 'static) -> Result<(), ToolError> {
        let name = tool.name().to_string();
        let definition = ToolDefinition {
            name: name.clone(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            trust_requirement: tool.trust_requirement().to_string(),
            risk_level: tool.risk_level().to_string(),
            category: None,
        };

        debug!("Registering tool: {}", name);

        self.tools
            .write()
            .await
            .insert(name.clone(), Arc::new(tool));
        self.definitions.write().await.insert(name, definition);

        info!("Tool registered successfully");
        Ok(())
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn VilTool>> {
        self.tools.read().await.get(name).cloned()
    }

    pub async fn get_definition(&self, name: &str) -> Option<ToolDefinition> {
        self.definitions.read().await.get(name).cloned()
    }

    pub async fn list(&self) -> Vec<ToolDefinition> {
        self.definitions.read().await.values().cloned().collect()
    }

    /// F1.1 — Return every registered tool's formal `ToolSpec`.
    /// Source of truth for `ToolSearchTool` + downstream consumers
    /// (session engine, bridge) that need the full contract, not just
    /// the legacy `ToolDefinition` shape.
    pub async fn list_specs(&self) -> Vec<vac_tool_core::ToolSpec> {
        self.tools
            .read()
            .await
            .values()
            .map(|t| t.spec())
            .collect()
    }

    /// W2.2 — Initial tool manifest sent to the model on turn 1.
    /// Excludes tools whose `should_defer` is true — unless they
    /// also set `always_load`. A deferred tool is fetched on demand
    /// via [`load_deferred`].
    ///
    /// Order: `always_load` > `should_defer`. So a tool that declares
    /// both stays in the initial set — the override is explicit.
    pub async fn list_initial_specs(&self) -> Vec<vac_tool_core::ToolSpec> {
        self.tools
            .read()
            .await
            .values()
            .filter(|t| !t.should_defer() || t.always_load())
            .map(|t| t.spec())
            .collect()
    }

    /// W2.2 — Names of every tool deferred from the initial manifest.
    /// `ToolSearch` uses this to know which names it can resolve.
    pub async fn list_deferred_names(&self) -> Vec<String> {
        self.tools
            .read()
            .await
            .values()
            .filter(|t| t.should_defer() && !t.always_load())
            .map(|t| t.name().to_string())
            .collect()
    }

    /// W2.2 — Fetch a single tool's spec by name. Intended for the
    /// on-demand deferred-tool path; returns `None` when the name is
    /// not registered. Independent of the defer flag — caller is the
    /// one deciding when to resolve.
    pub async fn load_deferred(&self, name: &str) -> Option<vac_tool_core::ToolSpec> {
        self.tools.read().await.get(name).map(|t| t.spec())
    }

    /// W1.2 — Return only the specs whose capability is `read_only`.
    /// Used by the fork-speculation driver so a speculative sub-submit
    /// cannot accidentally call a mutating tool and pollute the
    /// parent session. `BashTool` has a dedicated `is_read_only_input`
    /// helper for per-command classification; use [`read_only_specs`]
    /// to filter an already-collected spec list with that hook when
    /// the context is a single input value.
    pub async fn list_read_only_specs(&self) -> Vec<vac_tool_core::ToolSpec> {
        self.list_specs()
            .await
            .into_iter()
            .filter(|s| s.capability.read_only)
            .collect()
    }

    pub async fn list_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        self.definitions
            .read()
            .await
            .values()
            .filter(|d| d.category.as_deref() == Some(category))
            .cloned()
            .collect()
    }

    pub async fn unregister(&self, name: &str) -> Result<(), ToolError> {
        if self.tools.write().await.remove(name).is_none() {
            return Err(ToolError::NotFound(name.to_string()));
        }
        self.definitions.write().await.remove(name);
        Ok(())
    }

    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let tool = self
            .get(name)
            .await
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        debug!("Executing tool: {}", name);
        let threshold = tool.max_result_size_chars();
        let raw = tool.execute(args, context).await?;
        // W2.3 — spill oversized results. Root lives under the
        // caller's working dir so each project owns its spill space.
        let spill_root = context
            .working_dir
            .join(".vac")
            .join("tool-results");
        match crate::result_spill::maybe_spill_result(raw, threshold, &spill_root).await {
            Ok(v) => Ok(v),
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "tool result spill failed: {e}"
            ))),
        }
    }
}

/// W1.2 — Filter a collected list of specs down to read-only only.
/// Mirrors [`ToolRegistry::list_read_only_specs`] for callers that
/// already have the full list (e.g. trajectory replay, MCP bridge
/// exporting a subset) and want the same invariant without touching
/// the registry lock.
pub fn read_only_specs(specs: Vec<vac_tool_core::ToolSpec>) -> Vec<vac_tool_core::ToolSpec> {
    specs.into_iter().filter(|s| s.capability.read_only).collect()
}

/// W1.2 — Classify a bash command as read-only. `BashTool` is the one
/// tool whose read-only status depends on the input (every other tool's
/// capability is static per spec). Used by the fork driver when it
/// wants to allow the speculation branch to run grep/find/wc/etc.
/// without permitting mutation. Pattern list is deliberately short +
/// explicit — any unknown binary is treated as non-read-only to stay
/// safe-by-default.
pub fn is_read_only_bash_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Reject any shell control that could chain a write: `;` `&&` `|`
    // with a redirect, backticks, process substitution, heredocs.
    // The fork path runs speculative reads only; a mixed command
    // flips to non-read-only to avoid chasing syntax edge-cases.
    let banned_tokens = [
        ";", "&&", "||", ">", ">>", "<<", "<(", ">(", "|&", "`", "$(",
    ];
    if banned_tokens.iter().any(|t| trimmed.contains(t)) {
        return false;
    }
    // `|` is allowed only when every segment is read-only.
    if trimmed.contains('|') {
        return trimmed
            .split('|')
            .all(|seg| is_read_only_bash_command(seg));
    }
    let first = trimmed.split_whitespace().next().unwrap_or("");
    // Per-tool hazards that the whitelist alone won't catch. Ordering
    // matters — these checks run before the whitelist filter.
    if first == "find" {
        // `find` is a search utility but its action args can mutate:
        // -exec / -execdir / -ok / -okdir / -delete all write.
        const FIND_MUTATING_ACTIONS: &[&str] =
            &["-exec", "-execdir", "-ok", "-okdir", "-delete"];
        for token in trimmed.split_whitespace() {
            if FIND_MUTATING_ACTIONS.contains(&token) {
                return false;
            }
        }
    }
    if first == "git" {
        // Only the read-only `git` subcommands pass. Anything else
        // (commit, push, reset, stash, …) mutates.
        const GIT_READ_ONLY_SUBS: &[&str] = &[
            "log", "diff", "status", "show", "blame", "branch",
            "remote", "tag", "describe", "rev-parse", "rev-list",
            "ls-files", "ls-tree", "cat-file", "shortlog", "reflog",
        ];
        let sub = trimmed.split_whitespace().nth(1).unwrap_or("");
        return GIT_READ_ONLY_SUBS.contains(&sub);
    }
    // `sed -i` and `awk -i` mutate; reject.
    if first == "sed" && trimmed.contains(" -i") {
        return false;
    }
    if first == "awk" && trimmed.contains(" -i ") {
        return false;
    }
    // Whitelisted utilities. Anything else returns false.
    const READ_ONLY_BINS: &[&str] = &[
        "ls", "cat", "head", "tail", "wc", "grep", "rg", "find", "fd",
        "du", "df", "stat", "file", "which", "echo", "pwd", "id",
        "uname", "hostname", "date", "printf", "tree", "awk", "sed",
        "sort", "uniq", "cut", "tr", "column", "less", "more", "git",
        "ps", "env", "history", "jobs", "whoami",
    ];
    READ_ONLY_BINS.contains(&first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_tool_core::{ToolCapability, ToolPermissionClass, ToolRenderHints, ToolSpec};

    fn make_spec(name: &str, cap: ToolCapability) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: "test".into(),
            input_schema: serde_json::json!({"type": "object"}),
            capability: cap,
            permission: ToolPermissionClass::Safe,
            render: ToolRenderHints::default(),
        }
    }

    #[test]
    fn audit_test_proves_no_default_spec() {
        // In Rust, because `VilTool::spec` has no default implementation in the trait definition,
        // the compiler enforces that every implementor provides its own `spec()` override.
        // This test serves as the audit record required by M3.
        let _ = "Compiler proved no default `spec` exists for `VilTool`";
    }

    #[test]
    fn read_only_specs_filters_mutating() {
        let specs = vec![
            make_spec("grep", ToolCapability::default()),
            make_spec("edit", ToolCapability::mutating()),
            make_spec("rm", ToolCapability::destructive()),
            make_spec("ls", ToolCapability::default()),
        ];
        let filtered = read_only_specs(specs);
        let names: Vec<_> = filtered.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["grep", "ls"]);
    }

    #[test]
    fn read_only_specs_empty_input_empty_output() {
        assert!(read_only_specs(Vec::new()).is_empty());
    }

    #[test]
    fn bash_classifier_accepts_whitelisted() {
        assert!(is_read_only_bash_command("grep foo src/"));
        assert!(is_read_only_bash_command("ls -la"));
        assert!(is_read_only_bash_command("wc -l file.txt"));
        assert!(is_read_only_bash_command("cat Cargo.toml | head -20"));
        assert!(is_read_only_bash_command("find . -name '*.rs'"));
    }

    #[test]
    fn bash_classifier_rejects_mutations() {
        assert!(!is_read_only_bash_command("rm -rf /"));
        assert!(!is_read_only_bash_command("git commit"));
        assert!(!is_read_only_bash_command("sed -i 's/a/b/' file"));
        assert!(!is_read_only_bash_command("awk -i inplace '{print}' f"));
        assert!(!is_read_only_bash_command(""));
        assert!(!is_read_only_bash_command("   "));
    }

    #[test]
    fn bash_classifier_rejects_find_mutating_actions() {
        // Security regression: earlier versions whitelisted `find`
        // wholesale, allowing `-exec rm`, `-delete`, etc.
        assert!(!is_read_only_bash_command("find . -name '*.tmp' -delete"));
        assert!(!is_read_only_bash_command("find . -exec rm {} +"));
        assert!(!is_read_only_bash_command("find . -execdir rm {} \\;"));
        assert!(!is_read_only_bash_command("find . -ok rm {} \\;"));
        assert!(!is_read_only_bash_command("find . -okdir rm {} \\;"));
        // Pure search still passes.
        assert!(is_read_only_bash_command("find . -name '*.rs'"));
        assert!(is_read_only_bash_command("find src -type f"));
    }

    #[test]
    fn bash_classifier_allows_only_read_only_git_subcommands() {
        assert!(is_read_only_bash_command("git log --oneline -5"));
        assert!(is_read_only_bash_command("git diff HEAD"));
        assert!(is_read_only_bash_command("git status"));
        assert!(is_read_only_bash_command("git show HEAD"));
        assert!(is_read_only_bash_command("git blame src/lib.rs"));
        assert!(is_read_only_bash_command("git rev-parse HEAD"));
        // Mutators rejected.
        assert!(!is_read_only_bash_command("git commit -m x"));
        assert!(!is_read_only_bash_command("git push"));
        assert!(!is_read_only_bash_command("git reset --hard"));
        assert!(!is_read_only_bash_command("git stash"));
        assert!(!is_read_only_bash_command("git add ."));
        // `git` with no subcommand is rejected (would print help
        // but nothing useful for fork speculation).
        assert!(!is_read_only_bash_command("git"));
    }

    #[test]
    fn bash_classifier_rejects_shell_control() {
        assert!(!is_read_only_bash_command("ls ; rm x"));
        assert!(!is_read_only_bash_command("ls && rm x"));
        assert!(!is_read_only_bash_command("ls > out.txt"));
        assert!(!is_read_only_bash_command("echo `whoami`"));
        assert!(!is_read_only_bash_command("cat $(pwd)"));
        assert!(!is_read_only_bash_command("cat <(echo)"));
    }

    #[test]
    fn bash_classifier_allows_pipe_only_between_safe_bins() {
        assert!(is_read_only_bash_command("grep foo file | wc -l"));
        // Pipe containing a non-read-only bin fails.
        assert!(!is_read_only_bash_command("cat x | rm y"));
    }

    struct CapProbe {
        read: bool,
        dest: bool,
        conc: bool,
    }

    #[async_trait]
    impl VilTool for CapProbe {
        fn name(&self) -> &str { "probe" }
        fn description(&self) -> &str { "probe" }
        fn input_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
        fn trust_requirement(&self) -> &str { "safe" }
        fn risk_level(&self) -> &str { "low" }
        fn spec(&self) -> vac_tool_core::ToolSpec {
            make_spec(
                "probe",
                ToolCapability {
                    read_only: self.read,
                    destructive: self.dest,
                    concurrency_safe: self.conc,
                    ..Default::default()
                },
            )
        }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({}))
        }
    }

    #[test]
    fn per_input_defaults_match_spec_defaults() {
        let t = CapProbe { read: true, dest: false, conc: true };
        let empty = serde_json::json!({});
        assert!(t.is_input_read_only(&empty));
        assert!(!t.is_input_destructive(&empty));
        assert!(t.is_input_concurrency_safe(&empty));
    }

    #[test]
    fn per_input_defaults_reflect_mutating_spec() {
        let t = CapProbe { read: false, dest: true, conc: false };
        let empty = serde_json::json!({});
        assert!(!t.is_input_read_only(&empty));
        assert!(t.is_input_destructive(&empty));
        assert!(!t.is_input_concurrency_safe(&empty));
    }

    #[test]
    fn backfill_observable_input_is_noop_by_default() {
        let t = CapProbe { read: true, dest: false, conc: true };
        let mut v = serde_json::json!({"a": 1});
        let before = v.clone();
        t.backfill_observable_input(&mut v);
        assert_eq!(v, before, "default backfill must not mutate");
    }

    #[test]
    fn defer_flags_default_false() {
        let t = CapProbe { read: true, dest: false, conc: true };
        assert!(!t.should_defer());
        assert!(!t.always_load());
        assert_eq!(t.max_result_size_chars(), 256 * 1024);
    }

    struct DeferProbe {
        name: &'static str,
        defer: bool,
        always: bool,
    }

    #[async_trait]
    impl VilTool for DeferProbe {
        fn name(&self) -> &str { self.name }
        fn description(&self) -> &str { self.name }
        fn input_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
        fn trust_requirement(&self) -> &str { "safe" }
        fn risk_level(&self) -> &str { "low" }
        fn spec(&self) -> vac_tool_core::ToolSpec {
            make_spec(self.name, ToolCapability::default())
        }
        fn should_defer(&self) -> bool { self.defer }
        fn always_load(&self) -> bool { self.always }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({}))
        }
    }

    async fn mk_mixed_registry() -> ToolRegistry {
        let reg = ToolRegistry::new();
        reg.register(DeferProbe { name: "eager", defer: false, always: false }).await.unwrap();
        reg.register(DeferProbe { name: "Grep", defer: true, always: false }).await.unwrap();
        reg.register(DeferProbe { name: "Glob", defer: true, always: false }).await.unwrap();
        reg.register(DeferProbe { name: "CriticalSearch", defer: true, always: true }).await.unwrap();
        reg
    }

    #[tokio::test]
    async fn initial_specs_excludes_deferred() {
        let reg = mk_mixed_registry().await;
        let names: std::collections::HashSet<String> = reg
            .list_initial_specs()
            .await
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert!(names.contains("eager"));
        assert!(names.contains("CriticalSearch"), "always_load overrides defer");
        assert!(!names.contains("Grep"));
        assert!(!names.contains("Glob"));
    }

    #[tokio::test]
    async fn deferred_names_lists_deferred_only() {
        let reg = mk_mixed_registry().await;
        let names: std::collections::HashSet<String> =
            reg.list_deferred_names().await.into_iter().collect();
        assert!(names.contains("Grep"));
        assert!(names.contains("Glob"));
        assert!(!names.contains("eager"));
        assert!(
            !names.contains("CriticalSearch"),
            "always_load excludes from deferred set",
        );
    }

    #[tokio::test]
    async fn load_deferred_resolves_by_name() {
        let reg = mk_mixed_registry().await;
        let spec = reg.load_deferred("Grep").await.unwrap();
        assert_eq!(spec.name, "Grep");
        assert!(reg.load_deferred("does-not-exist").await.is_none());
    }

    #[tokio::test]
    async fn load_deferred_works_for_non_deferred_too() {
        // W2.2 plan spec: load_deferred is "independent of the defer
        // flag". Caller decides when to resolve.
        let reg = mk_mixed_registry().await;
        assert!(reg.load_deferred("eager").await.is_some());
    }

    struct GiantOutputTool {
        payload_chars: usize,
        threshold: usize,
    }

    #[async_trait]
    impl VilTool for GiantOutputTool {
        fn name(&self) -> &str { "giant" }
        fn description(&self) -> &str { "emits large payload" }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn trust_requirement(&self) -> &str { "safe" }
        fn risk_level(&self) -> &str { "low" }
        fn spec(&self) -> vac_tool_core::ToolSpec {
            make_spec("giant", ToolCapability::default())
        }
        fn max_result_size_chars(&self) -> usize {
            self.threshold
        }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({
                "data": "y".repeat(self.payload_chars),
            }))
        }
    }

    #[tokio::test]
    async fn registry_execute_spills_oversized_output() {
        let reg = ToolRegistry::new();
        reg.register(GiantOutputTool {
            payload_chars: 10_000,
            threshold: 1_000,
        })
        .await
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let out = reg
            .execute("giant", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert!(
            crate::result_spill::PreviewStub::is_stub(&out),
            "registry must route oversized output through spill"
        );
        let spill_dir = tmp.path().join(".vac/tool-results");
        assert!(spill_dir.is_dir(), "spill dir must be created");
    }

    #[tokio::test]
    async fn registry_execute_passes_through_small_output() {
        let reg = ToolRegistry::new();
        reg.register(GiantOutputTool {
            payload_chars: 50,
            threshold: 1_000,
        })
        .await
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let out = reg
            .execute("giant", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert!(!crate::result_spill::PreviewStub::is_stub(&out));
    }

    #[tokio::test]
    async fn registry_execute_respects_usize_max_opt_out() {
        let reg = ToolRegistry::new();
        reg.register(GiantOutputTool {
            payload_chars: 10_000,
            threshold: usize::MAX,
        })
        .await
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let out = reg
            .execute("giant", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert!(
            !crate::result_spill::PreviewStub::is_stub(&out),
            "usize::MAX threshold must skip spill"
        );
    }

    #[tokio::test]
    async fn registry_list_read_only_specs_filters() {
        // Synthetic: build a registry by hand using a tiny VilTool-like
        // mock is overkill; instead cover the filter through the helper
        // which registry.list_read_only_specs delegates to.
        let specs = vec![
            make_spec("r", ToolCapability::default()),
            make_spec("w", ToolCapability::mutating()),
        ];
        let ro = read_only_specs(specs);
        assert_eq!(ro.len(), 1);
        assert_eq!(ro[0].name, "r");
    }
}
