//! VAC CLI — Vastar Agentic CLI entry point.

pub mod boot;
mod commands;
mod io;
mod output;
mod telemetry;

use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

const VAC_AFTER_HELP: &str = "\
Command groups:
  Run:             run, exec, interactive, autopilot, resume
  Config:          config, auth, rulebook, isolation, sandbox, migrate, doctor
  Trace & Export:  export, import, observe, explain, why, status
  Interop:         acp, mcp
  VIL Tooling:     init, vil, runtime, restore

Run `vac <COMMAND> --help` for details on a specific command.";

#[derive(Parser)]
#[command(
    name = "vac",
    version,
    about = "VAC — Vastar Agentic CLI: Autonomous development powered by VIL Engine",
    long_about = None,
    after_help = VAC_AFTER_HELP,
    after_long_help = VAC_AFTER_HELP,
)]
struct Cli {
    #[arg(short = 'C', long, global = true)]
    project: Option<PathBuf>,
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[arg(long, default_value = "text", global = true)]
    format: String,
    #[arg(long, env = "VAC_LOG_FORMAT", default_value = "text", global = true)]
    log_format: String,
    #[arg(long, env = "VAC_OTEL_ENDPOINT", global = true)]
    otel_endpoint: Option<String>,
    #[arg(long, env = "VAC_METRICS_ADDR", global = true)]
    metrics_addr: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // ----- Run -----
    /// Execute a single task via agent swarm
    #[command(next_help_heading = "Run")]
    Run {
        task: String,
        #[arg(short, long, default_value = "normal")]
        priority: String,
        #[arg(long, default_value = "default")]
        profile: String,
        #[arg(long)]
        approve: bool,
        #[arg(short, long)]
        target: Vec<String>,
        /// Token budget for this submit
        #[arg(long)]
        budget_tokens: Option<u64>,
        /// Local inference backend
        #[arg(long)]
        backend: Option<String>,
    },
    /// Execute a command (Codex-grade CLI grammar)
    #[command(next_help_heading = "Run")]
    Exec {
        /// The prompt to execute
        prompt: String,
        /// Ephemeral run (disables trajectory)
        #[arg(long)]
        ephemeral: bool,
        /// Override LLM provider for this command (sets VAC_LLM_DEFAULT_PROVIDER).
        #[arg(long, value_name = "NAME")]
        provider: Option<String>,
        /// Sandbox mode (Codex-grade spelling).
        #[arg(long)]
        sandbox: Option<String>,
    },
    /// Interactive REPL mode
    #[command(next_help_heading = "Run", visible_alias = "chat")]
    Interactive {
        #[arg(long)]
        resume: bool,
        /// Override LLM provider for this process (sets VAC_LLM_DEFAULT_PROVIDER).
        #[arg(long, value_name = "NAME")]
        provider: Option<String>,
        /// Explicitly set the user sandbox mode: read-only, workspace-write, or danger-full-access.
        #[arg(long)]
        sandbox_mode: Option<String>,
        /// Record all user input to a JSONL file under <dir> for later replay (PR-T18).
        #[arg(long, value_name = "DIR")]
        record: Option<PathBuf>,
        /// Replay a previously recorded JSONL input file instead of reading the terminal (PR-T18).
        #[arg(long, value_name = "FILE", conflicts_with = "record")]
        replay: Option<PathBuf>,
    },
    /// Autopilot daemon — 24/7 autonomous runtime with VIL policy enforcement
    #[command(next_help_heading = "Run")]
    Autopilot {
        #[command(subcommand)]
        action: AutopilotAction,
    },
    /// Proactive assistant - scans signal pipelines for patterns and suggests tasks
    #[command(next_help_heading = "Run")]
    Assistant {
        #[arg(long)]
        session: Option<String>,
    },
    /// Remote deep planner - offloads long planning sessions to remote model
    #[command(next_help_heading = "Run")]
    Plan {
        /// The planning prompt
        prompt: String,
        /// Remote endpoint URI
        #[arg(long)]
        remote: Option<String>,
    },
    /// Apply a generated plan
    #[command(next_help_heading = "Run")]
    PlanApply {
        /// Plan ID to apply
        plan_id: String,
    },
    /// Resume from checkpoint
    #[command(next_help_heading = "Run")]
    Resume { checkpoint: PathBuf },
    /// Drive one submit through vac_session_engine (Trae-style
    /// one-shot: trajectory-first, provider-pluggable, isolation
    /// opt-in)
    #[command(next_help_heading = "Run")]
    SessionRun {
        input: String,
        /// Provider adapter (today only `mock`/`echo`; real providers
        /// land with the adapter registry).
        #[arg(long, default_value = "mock")]
        provider: String,
        /// Disable trajectory persistence. When set, the transcript
        /// JSONL is routed to a tempdir and unlinked at end of run
        /// instead of landing under `<root>/.vac/sessions/`. On by
        /// default for research-friendly replay.
        #[arg(long = "no-trajectory", default_value_t = false)]
        no_trajectory: bool,
        /// [preview] Record a docker image in submit metadata so
        /// replay harnesses see the isolation intent. Tool execution
        /// still runs on host until IsolationManager wires into
        /// session-engine. Incompatible with --no-trajectory.
        #[arg(long, value_name = "IMAGE")]
        docker: Option<String>,
    },

    // ----- Config -----
    /// Manage configuration
    #[command(next_help_heading = "Config")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage authentication
    #[command(next_help_heading = "Config")]
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Manage VIL-native rulebooks (SOP, playbooks, governance constraints)
    #[command(next_help_heading = "Config", visible_alias = "config-rulebook")]
    Rulebook {
        #[command(subcommand)]
        action: RulebookAction,
    },
    /// Execution boundary and isolation management
    #[command(next_help_heading = "Config", visible_alias = "config-isolation")]
    Isolation {
        #[command(subcommand)]
        action: IsolationAction,
    },
    /// Migrate .vac/ schema to the latest version
    #[command(next_help_heading = "Config")]
    Migrate,
    /// Sandbox management (Codex-grade CLI grammar)
    #[command(next_help_heading = "Config")]
    Sandbox {
        #[command(subcommand)]
        action: SandboxAction,
    },
    /// Check VAC subsystem readiness
    #[command(next_help_heading = "Config")]
    Doctor {
        #[arg(long)]
        strict: bool,
        #[arg(long)]
        fix: bool,
        #[arg(long, short)]
        interactive: bool,
    },

    // ----- Trace & Export -----
    /// Export session artifact
    #[command(next_help_heading = "Trace & Export")]
    Export {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long, default_value = "vac-cbor")]
        format: String,
        #[arg(long)]
        sign: bool,
    },
    /// Import session bundle into current project
    #[command(next_help_heading = "Trace & Export")]
    Import {
        input: PathBuf,
        #[arg(short, long, default_value = "bundle-json")]
        format: String,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        require_signed: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        overwrite_session: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        trust_approvals: bool,
        #[arg(long = "no-redact", action = clap::ArgAction::SetFalse, default_value_t = true)]
        redact: bool,
    },
    /// Inspect vac_signal rewind databases. Tail captured output streams
    /// that were persisted via `SignalRegistry::persist_to_rewind`, or
    /// list their summaries. Requires the signal layer's `rewind` feature
    /// to be active at runtime.
    #[command(next_help_heading = "Trace & Export", subcommand)]
    Signal(SignalCommand),
    /// List or inspect bundled skills (W3). Skills are named workflows
    /// the agent invokes via `SkillTool`.
    #[command(next_help_heading = "Tools & Skills", subcommand)]
    Skills(SkillsCommand),

    // ── W8 commands ────────────────────────────────────────────────
    /// Quick pre-commit snapshot of local state. Branch, uncommitted
    /// file count, last commit, next-action hint.
    #[command(next_help_heading = "Review")]
    Advisor {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Lint the unstaged diff for common auto-fixable rules (TODO,
    /// leftover println, `.unwrap()` in prod, dbg!).
    #[command(next_help_heading = "Review")]
    AutofixPr {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Scan the last commit for code smells (panic, unreachable,
    /// expect-in-prod, hard-exit).
    #[command(next_help_heading = "Review")]
    Bughunter {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Scan the staged diff for secret-shaped strings.
    #[command(next_help_heading = "Review", name = "security-review")]
    SecurityReview {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Surface large files + slow-test markers for the perf backlog.
    #[command(next_help_heading = "Review", name = "perf-issue")]
    PerfIssue {
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Print install steps for the VAC GitHub App.
    #[command(next_help_heading = "Integrations", name = "install-github-app")]
    InstallGithubApp,
    /// Print install steps for the VAC Slack App.
    #[command(next_help_heading = "Integrations", name = "install-slack-app")]
    InstallSlackApp,
    /// Scan `.vac/plugins/` for operator-dropped plugins.
    #[command(next_help_heading = "Integrations", name = "reload-plugins")]
    ReloadPlugins,
    /// Teleport: list recent sessions, or host/attach a remote SSE bridge.
    #[command(next_help_heading = "Integrations")]
    Teleport {
        /// Host mode: start an SSE server and mint a teleport JWT.
        #[arg(long)]
        serve: bool,
        /// Bind address for host mode (default 127.0.0.1:9042).
        #[arg(long, default_value = "127.0.0.1:9042")]
        bind: String,
        /// Short operator label — surfaces in the activity row on attach.
        #[arg(long, default_value = "teleport")]
        label: String,
        /// Allow non-loopback binds (HTTP only today; TLS is TODO).
        #[arg(long)]
        insecure: bool,
        /// Attach mode: connect to a host using this bearer token.
        #[arg(long)]
        attach: Option<String>,
        /// Base URL of the host (e.g. http://192.168.1.10:9042).
        #[arg(long, default_value = "http://127.0.0.1:9042")]
        url: String,
    },

    /// Dump the last tool-call line from the newest transcript.
    #[command(next_help_heading = "Diagnostics", name = "debug-tool-call")]
    DebugToolCall,
    /// Heap / RSS snapshot of the current process.
    #[command(next_help_heading = "Diagnostics")]
    Heapdump,
    /// One-line statusline preview — branch, session count, cwd.
    #[command(next_help_heading = "Diagnostics")]
    Statusline,
    /// Delight.
    #[command(next_help_heading = "Diagnostics", name = "good-claude")]
    GoodClaude,
    /// Post-install first-steps checklist (U8 onboarding polish).
    /// Prints what's configured vs missing under `.vac/` and suggests
    /// concrete next commands for each gap.
    #[command(next_help_heading = "Diagnostics")]
    Onboard,

    /// Tail the most recent session transcript.
    #[command(next_help_heading = "Plan & Memory")]
    Thinkback {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print a plan skeleton for a goal (offline; `vac plan --remote`
    /// is the LLM-backed version).
    #[command(next_help_heading = "Plan & Memory")]
    Ultraplan { goal: String },
    /// Cycle `environment_mode` through host → isolated → restricted-
    /// offline → trusted-networked → host.
    #[command(next_help_heading = "Plan & Memory", name = "sandbox-toggle")]
    SandboxToggle,
    /// Show session resume candidates newest-first.
    #[command(next_help_heading = "Plan & Memory")]
    Rewind {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Observe recent trajectory artifacts
    #[command(next_help_heading = "Trace & Export")]
    Observe {
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
    /// Dump agent-decision records from a trace file (or the most recent
    /// one if no path is given). Useful for Trae-style offline evaluation
    /// of agent loops.
    #[command(next_help_heading = "Trace & Export")]
    Decisions {
        /// Path to a trace JSON file. If omitted, the most recent trace
        /// under `.vac/traces/` is used.
        #[arg(value_name = "TRACE")]
        path: Option<std::path::PathBuf>,
    },
    /// Score an agent-decision sequence against a task outcome.
    /// Prints a 0..=100 decision score plus per-decision weights.
    #[command(next_help_heading = "Trace & Export")]
    Eval {
        /// Trace JSON file (defaults to most recent under .vac/traces/).
        #[arg(value_name = "TRACE")]
        path: Option<std::path::PathBuf>,
        /// Whether the task ultimately succeeded.
        #[arg(long, default_value_t = true)]
        succeeded: bool,
        /// Task wall-clock duration in milliseconds.
        #[arg(long, default_value_t = 0u64)]
        duration_ms: u64,
        /// Compare against a golden decision file (JSON array of
        /// { chosen, rejected, rationale } records). Prints match-rate %.
        #[arg(long)]
        golden: Option<std::path::PathBuf>,
        /// Load VacConfig::minimal() (no trace, no memory, no MCP, no
        /// policy gate). Intended for research / replay runs where
        /// side-effects would contaminate comparison.
        #[arg(long, default_value_t = false)]
        minimal: bool,
    },
    /// Explain a trajectory by id, label, or file path
    #[command(next_help_heading = "Trace & Export")]
    Explain {
        #[arg(value_name = "TARGET")]
        target: Option<String>,
    },
    /// Explain why a file changed
    #[command(next_help_heading = "Trace & Export")]
    Why {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_name = "TARGET")]
        trajectory: Option<String>,
    },
    /// Show engine status
    #[command(next_help_heading = "Trace & Export")]
    Status,

    // ----- Interop -----
    /// Start ACP editor-facing agent server
    #[command(next_help_heading = "Interop")]
    Acp {
        #[arg(long, default_value = "4123")]
        port: u16,
    },
    /// Model Context Protocol (MCP) server management
    #[command(next_help_heading = "Interop")]
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    // ----- VIL Tooling -----
    /// Initialize VIL project context
    #[command(next_help_heading = "VIL Tooling")]
    Init {
        #[arg(short, long)]
        force: bool,
    },
    /// Manage the external VIL binary
    #[command(next_help_heading = "VIL Tooling")]
    Vil {
        #[command(subcommand)]
        action: VilAction,
    },
    /// Background runtime management
    #[command(next_help_heading = "VIL Tooling")]
    Runtime {
        #[command(subcommand)]
        action: RuntimeAction,
    },
    /// Restore a file or an entire submit from backup
    #[command(next_help_heading = "VIL Tooling")]
    Restore {
        /// File to restore
        #[arg(required_unless_present = "submit")]
        file: Option<PathBuf>,
        /// Reverses every file touched in that submit
        #[arg(long)]
        submit: Option<uuid::Uuid>,
    },
    /// Build persistent BM25 index of the workspace for fast search
    #[command(next_help_heading = "System")]
    Ingest,
}

#[derive(Subcommand)]
enum ConfigAction {
    Show,
    Set {
        key: String,
        value: String,
    },
    AddProvider {
        name: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        api_key_env: String,
        #[arg(long)]
        base_url: Option<String>,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    Login {
        #[arg(long)]
        token: Option<String>,
    },
    /// G2 — browser-redirect OAuth with PKCE. Opens the authorize
    /// URL, listens on a loopback port, exchanges the code for a
    /// token, stores it under `TokenCache::default_root()`.
    Oauth {
        /// Provider key (must be registered in commands/auth.rs).
        provider: String,
        /// Override client_id (otherwise provider default).
        #[arg(long)]
        client_id: Option<String>,
        /// Override authorize URL.
        #[arg(long)]
        auth_url: Option<String>,
        /// Override token URL.
        #[arg(long)]
        token_url: Option<String>,
    },
    Status,
    Logout,
}

#[derive(Subcommand)]
enum RulebookAction {
    /// List all loaded rulebooks with constraint summary
    List,
    /// Validate rulebooks against VIL semantic contracts
    Validate,
    /// Apply a rulebook from a markdown file (YAML frontmatter supported, backs up existing)
    Apply {
        /// Path to the markdown rulebook file
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum VilAction {
    /// Initialize a VIL project
    Init,
    /// Run the VIL development loop
    Dev,
    /// Generate VIL artifacts
    Gen {
        /// Artifact template to generate, e.g. `handler`
        entity: String,
        /// VIL semantic kind for the generated artifact
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// Execution mode for the generated artifact
        #[arg(long = "execution-mode", value_name = "MODE")]
        execution_mode: String,
        /// Output name for the generated artifact
        #[arg(long)]
        name: String,
    },
    /// Deploy VIL artifacts
    Deploy {
        /// Optional deploy target
        target: Option<String>,
    },
}

#[derive(Subcommand)]
enum RuntimeAction {
    /// Show runtime status
    Status,
    /// List queued jobs
    Jobs,
    /// Start the background scheduler (attaches live engine)
    Start,
    /// Cancel a queued or running job
    Cancel {
        /// The UUID of the job to cancel
        id: uuid::Uuid,
    },
    /// Retry a failed or cancelled job
    Retry {
        /// The UUID of the job to retry
        id: uuid::Uuid,
    },
    /// Inspect a specific job's details
    Inspect {
        /// The UUID of the job to inspect
        id: uuid::Uuid,
    },
}

#[derive(Subcommand)]
enum SandboxAction {
    /// Show sandbox status
    Status,
    /// Run diagnostics for the sandbox environment
    Doctor,
}

#[derive(Subcommand)]
enum IsolationAction {
    /// Show effective isolation configuration
    Status,
    /// Print isolation log
    Logs,
    /// Run a command inside the configured isolated environment
    Run {
        /// Force TTY on or off. If omitted, VAC infers from execution_environment.
        #[arg(long)]
        tty: Option<bool>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Wrap an arbitrary command or re-exec VAC interactively inside isolation
    Wrap {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Clear the isolation log
    ClearLogs,
    /// Run diagnostics for the isolation environment
    Doctor,
}

#[derive(Subcommand)]
enum McpAction {
    /// List all configured MCP servers with trust badges
    List,
    /// Show MCP status summary
    Status,
}

#[derive(Subcommand)]
enum AutopilotAction {
    /// Start autopilot daemon. Requires --execute to confirm intent;
    /// otherwise does a dry-run (config sanity-check, no process spawn).
    Up {
        /// Actually spawn the background daemon. Without this flag,
        /// `up` only validates config + prints the plan.
        #[arg(long, default_value_t = false)]
        execute: bool,
    },
    /// Stop autopilot daemon gracefully via SIGTERM
    Down,
    /// Show autopilot status, mode, and log path
    Status,
    /// Manage scheduled tasks
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },
    #[command(hide = true)]
    Run,
}

#[derive(Subcommand)]
pub enum ScheduleAction {
    /// List all scheduled tasks
    List,
    /// Add a scheduled task
    Add {
        /// Schedule ID
        id: String,
        /// Cron expression
        #[arg(long)]
        cron: String,
        /// Task description
        #[arg(long)]
        task: String,
        /// Optional rulebook ID
        #[arg(long)]
        rulebook: Option<String>,
    },
    /// Remove a scheduled task
    Remove {
        /// Schedule ID to remove
        id: String,
    },
}

struct EnvVarGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => unsafe {
                std::env::set_var(self.key, v);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

struct FileRestoreGuard {
    path: PathBuf,
    prev: Option<String>,
}

impl FileRestoreGuard {
    fn capture(path: PathBuf) -> Self {
        let prev = std::fs::read_to_string(&path).ok();
        Self { path, prev }
    }
}

impl Drop for FileRestoreGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(content) => {
                let _ = std::fs::write(&self.path, content);
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

fn list_session_transcripts(project_root: &Path) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let dir = project_root.join(".vac").join("sessions");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            out.insert(path);
        }
    }
    out
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli =
        crate::boot::boot_profile().record("parse_args", crate::boot::BootPhase::Critical, || {
            Cli::parse()
        });
    let interactive_mode = matches!(&cli.command, Commands::Interactive { .. });

    // Telemetry init — previously skipped entirely for interactive
    // mode to keep stderr clean under the TUI's alternate screen.
    // Dogfood fix: always init so `VAC_TUI_LOG=<path>` file-based
    // tracing works under interactive too. Interactive mode bypasses
    // stderr via the `tui_mode` flag so the TUI repaint stays clean;
    // file layer still fires when VAC_TUI_LOG is set.
    crate::boot::boot_profile().record(
        "telemetry_init",
        crate::boot::BootPhase::Critical,
        || {
            telemetry::init(
                cli.verbose,
                &cli.log_format,
                cli.otel_endpoint.as_deref(),
                cli.metrics_addr.as_deref(),
                interactive_mode,
            )
        },
    )?;

    let project_root = cli.project.clone().unwrap_or_else(|| {
        #[allow(clippy::expect_used)]
        std::env::current_dir().expect("Failed to get current directory")
    });

    let result = crate::boot::boot_profile()
        .record_async(
            "execute_command",
            crate::boot::BootPhase::Critical,
            async move {
                match cli.command {
                    Commands::Doctor {
                        strict,
                        fix,
                        interactive,
                    } => {
                        commands::doctor::execute(
                            project_root,
                            &cli.format,
                            strict,
                            fix,
                            interactive,
                        )
                        .await?
                    }
                    Commands::Init { force } => {
                        commands::init::execute(project_root, force).await?
                    }
                    Commands::Run {
                        task,
                        priority,
                        profile,
                        approve,
                        target,
                        budget_tokens,
                        backend,
                    } => {
                        commands::run::execute(
                            project_root,
                            task,
                            priority,
                            profile,
                            approve,
                            target,
                            budget_tokens,
                            backend,
                        )
                        .await?;
                    }
                    Commands::Exec {
                        prompt,
                        ephemeral,
                        sandbox,
                        provider,
                    } => {
                        let _provider_guard = provider
                            .as_deref()
                            .map(|p| EnvVarGuard::set(vil_llm::config::ENV_DEFAULT_PROVIDER, p));

                        let _sandbox_guard = if let Some(mode) = sandbox.as_deref() {
                            let sandbox_mode = vac_core::config::UserSandboxMode::parse_user(mode)
                                .map_err(|e| anyhow::anyhow!("invalid --sandbox: {e}"))?;
                            if sandbox_mode != vac_core::config::UserSandboxMode::DangerFullAccess {
                                let config_path = project_root.join(".vac/config.toml");
                                let guard = FileRestoreGuard::capture(config_path);
                                let mut config = vac_core::VacConfig::load_with_fallback(&project_root)?;
                                config.runtime.sandbox_mode = sandbox_mode;
                                sandbox_mode.apply_to_runtime(&mut config.runtime);
                                if config
                                    .runtime
                                    .container_image
                                    .as_deref()
                                    .unwrap_or_default()
                                    .trim()
                                    .is_empty()
                                {
                                    anyhow::bail!(
                                        "sandbox '{}' requires runtime.container_image to be set (see .vac/config.toml)",
                                        sandbox_mode.as_cli_str()
                                    );
                                }
                                config.validate()?;
                                vac_core::VacConfig::save(&project_root, &config)?;
                                Some(guard)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let sessions_before = if ephemeral {
                            Some(list_session_transcripts(&project_root))
                        } else {
                            None
                        };
                        let res = commands::run::execute(
                            project_root.clone(),
                            prompt,
                            "normal".to_string(),
                            "default".to_string(),
                            false,
                            Vec::new(),
                            None,
                            None,
                        )
                        .await;
                        if let Some(before) = sessions_before {
                            let after = list_session_transcripts(&project_root);
                            for path in after.difference(&before) {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                        res?
                    }
                    Commands::Interactive {
                        resume,
                        provider,
                        sandbox_mode,
                        record,
                        replay,
                    } => {
                        let _provider_guard = provider
                            .as_deref()
                            .map(|p| EnvVarGuard::set(vil_llm::config::ENV_DEFAULT_PROVIDER, p));
                        commands::interactive::execute(
                            project_root,
                            resume,
                            sandbox_mode,
                            record,
                            replay,
                        )
                        .await?
                    }
                    Commands::Assistant { session } => {
                        commands::assistant::execute(project_root, session).await?
                    }
                    Commands::Plan { prompt, remote } => {
                        commands::plan::execute(project_root, prompt, remote).await?
                    }
                    Commands::PlanApply { plan_id } => {
                        commands::plan::execute_apply(project_root, plan_id).await?
                    }
                    Commands::Resume { checkpoint } => {
                        commands::resume::execute(project_root, checkpoint).await?
                    }
                    Commands::SessionRun {
                        input,
                        provider,
                        no_trajectory,
                        docker,
                    } => {
                        let provider_kind = commands::session::ProviderKind::parse(&provider)
                            .map_err(|e| anyhow::anyhow!("invalid --provider: {e}"))?;
                        let opts = commands::session::SessionRunOptions {
                            input,
                            provider: provider_kind,
                            trajectory: !no_trajectory,
                            docker_image: docker,
                        };
                        commands::session::execute(project_root, opts).await?
                    }
                    Commands::Restore { file, submit } => {
                        commands::restore::execute(project_root, file, submit).await?
                    }
                    Commands::Status => {
                        commands::status::execute(project_root, &cli.format).await?
                    }
                    Commands::Signal(cmd) => {
                        commands::signal::dispatch(project_root, &cli.format, cmd).await?
                    }
                    Commands::Skills(cmd) => match cmd {
                        SkillsCommand::List => commands::skills::execute_list().await?,
                        SkillsCommand::Show { name } => {
                            commands::skills::execute_show(name).await?
                        }
                    },

                    // ── W8 dispatch ───────────────────────────────────────────
                    Commands::Advisor { format } => {
                        commands::review::advisor(
                            project_root,
                            commands::review::ReviewFormat::from_str(&format),
                        )
                        .await?
                    }
                    Commands::AutofixPr { format } => {
                        commands::review::autofix_pr(
                            project_root,
                            commands::review::ReviewFormat::from_str(&format),
                        )
                        .await?
                    }
                    Commands::Bughunter { format } => {
                        commands::review::bughunter(
                            project_root,
                            commands::review::ReviewFormat::from_str(&format),
                        )
                        .await?
                    }
                    Commands::SecurityReview { format } => {
                        commands::review::security_review(
                            project_root,
                            commands::review::ReviewFormat::from_str(&format),
                        )
                        .await?
                    }
                    Commands::PerfIssue { format } => {
                        commands::review::perf_issue(
                            project_root,
                            commands::review::ReviewFormat::from_str(&format),
                        )
                        .await?
                    }
                    Commands::InstallGithubApp => {
                        commands::integrations::install_github_app(project_root).await?
                    }
                    Commands::InstallSlackApp => {
                        commands::integrations::install_slack_app(project_root).await?
                    }
                    Commands::ReloadPlugins => {
                        commands::integrations::reload_plugins(project_root).await?
                    }
                    Commands::Teleport {
                        serve,
                        bind,
                        label,
                        insecure,
                        attach,
                        url,
                    } => {
                        if serve {
                            commands::teleport::teleport_serve(project_root, bind, label, insecure)
                                .await?
                        } else if let Some(token) = attach {
                            commands::teleport::teleport_attach(token, url).await?
                        } else {
                            // Legacy behaviour: list recent sessions.
                            commands::integrations::teleport(project_root).await?
                        }
                    }
                    Commands::DebugToolCall => {
                        commands::diagnostics::debug_tool_call(project_root).await?
                    }
                    Commands::Heapdump => commands::diagnostics::heapdump(project_root).await?,
                    Commands::Statusline => commands::diagnostics::statusline(project_root).await?,
                    Commands::GoodClaude => {
                        commands::diagnostics::good_claude(project_root).await?
                    }
                    Commands::Onboard => commands::onboard::execute(project_root).await?,
                    Commands::Thinkback { limit } => {
                        commands::plan_memory::thinkback(project_root, limit).await?
                    }
                    Commands::Ultraplan { goal } => {
                        commands::plan_memory::ultraplan(project_root, goal).await?
                    }
                    Commands::SandboxToggle => {
                        commands::plan_memory::sandbox_toggle(project_root).await?
                    }
                    Commands::Rewind { limit } => {
                        commands::plan_memory::rewind(project_root, limit).await?
                    }
                    Commands::Observe { limit } => {
                        commands::trajectory::observe(project_root, &cli.format, limit).await?
                    }
                    Commands::Decisions { path } => {
                        commands::trajectory::decisions(project_root, &cli.format, path).await?
                    }
                    Commands::Eval {
                        path,
                        succeeded,
                        duration_ms,
                        golden,
                        minimal,
                    } => {
                        commands::trajectory::eval(
                            project_root,
                            &cli.format,
                            path,
                            succeeded,
                            duration_ms,
                            golden,
                            minimal,
                        )
                        .await?
                    }
                    Commands::Explain { target } => {
                        commands::trajectory::explain(project_root, &cli.format, target).await?
                    }
                    Commands::Why { path, trajectory } => {
                        commands::trajectory::why(project_root, &cli.format, path, trajectory)
                            .await?
                    }
                    Commands::Config { action } => {
                        commands::config::execute(project_root, action).await?
                    }
                    Commands::Auth { action } => commands::auth::execute(action).await?,
                    Commands::Export {
                        output,
                        format,
                        sign,
                    } => {
                        commands::export::execute(project_root, output, format, sign).await?;
                    }
                    Commands::Import {
                        input,
                        format,
                        require_signed,
                        overwrite_session,
                        trust_approvals,
                        redact,
                    } => {
                        commands::import::execute(
                            project_root,
                            input,
                            format,
                            require_signed,
                            overwrite_session,
                            trust_approvals,
                            redact,
                        )
                        .await?;
                    }
                    Commands::Rulebook { action } => match action {
                        RulebookAction::List => {
                            commands::rulebook::execute_list(project_root).await?
                        }
                        RulebookAction::Validate => {
                            commands::rulebook::execute_validate(project_root).await?
                        }
                        RulebookAction::Apply { path } => {
                            commands::rulebook::execute_apply(project_root, path).await?
                        }
                    },
                    Commands::Vil { action } => {
                        commands::vil::execute(project_root, &cli.format, action).await?;
                    }
                    Commands::Acp { port } => commands::acp::execute(project_root, port).await?,
                    Commands::Runtime { action } => match action {
                        RuntimeAction::Status => {
                            commands::runtime::execute_status(project_root, &cli.format).await?
                        }
                        RuntimeAction::Jobs => {
                            commands::runtime::execute_jobs(project_root, &cli.format).await?
                        }
                        RuntimeAction::Start => {
                            commands::runtime::execute_start(project_root).await?
                        }
                        RuntimeAction::Cancel { id } => {
                            commands::runtime::execute_cancel(project_root, id).await?
                        }
                        RuntimeAction::Retry { id } => {
                            commands::runtime::execute_retry(project_root, id).await?
                        }
                        RuntimeAction::Inspect { id } => {
                            commands::runtime::execute_inspect(project_root, id, &cli.format)
                                .await?
                        }
                    },
                    Commands::Isolation { action } => match action {
                        IsolationAction::Status => {
                            commands::isolation::execute_status(project_root, &cli.format).await?
                        }
                        IsolationAction::Logs => {
                            commands::isolation::execute_logs(project_root).await?
                        }
                        IsolationAction::Run { tty, command } => {
                            commands::isolation::execute_run(project_root, command, tty).await?
                        }
                        IsolationAction::Wrap { command } => {
                            commands::isolation::execute_wrap(project_root, command).await?
                        }
                        IsolationAction::ClearLogs => {
                            commands::isolation::execute_clear_logs(project_root).await?
                        }
                        IsolationAction::Doctor => {
                            commands::isolation::execute_doctor(project_root, &cli.format).await?
                        }
                    },
                    Commands::Sandbox { action } => match action {
                        SandboxAction::Status => {
                            commands::isolation::execute_status(project_root, &cli.format).await?
                        }
                        SandboxAction::Doctor => {
                            commands::isolation::execute_doctor(project_root, &cli.format).await?
                        }
                    },
                    Commands::Mcp { action } => match action {
                        McpAction::List => commands::mcp::list(&project_root)?,
                        McpAction::Status => commands::mcp::status(&project_root).await?,
                    },
                    Commands::Autopilot { action } => match action {
                        AutopilotAction::Up { execute } => {
                            commands::autopilot::execute_up(project_root, execute).await?
                        }
                        AutopilotAction::Down => {
                            commands::autopilot::execute_down(project_root).await?
                        }
                        AutopilotAction::Status => {
                            commands::autopilot::execute_status(project_root, &cli.format).await?
                        }
                        AutopilotAction::Schedule { action } => {
                            commands::autopilot::execute_schedule(project_root, action).await?
                        }
                        AutopilotAction::Run => {
                            commands::autopilot::execute_run(project_root).await?
                        }
                    },
                    Commands::Migrate => {
                        commands::migrate::execute(project_root).await?;
                    }
                    Commands::Ingest => {
                        commands::ingest::execute(project_root).await?;
                    }
                }

                Ok(())
            },
        )
        .await;

    crate::boot::boot_profile().print_if_requested();
    result
}

/// W3 — skills introspection surface.
#[derive(Subcommand, Debug)]
pub enum SkillsCommand {
    /// List bundled skills with one-line descriptions.
    List,
    /// Show a specific skill: description + JSON schema.
    Show {
        /// Skill name (e.g. "verify", "batch").
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SignalCommand {
    /// List rewind-database files under `.vac/signal/`.
    List,
    /// Tail a stream from a rewind database.
    Tail {
        /// Path to the rewind SQLite database.
        #[arg(value_name = "DB")]
        db_path: std::path::PathBuf,
        /// Stream id (e.g. "vil_dev", "shell:0:<uuid>").
        #[arg(long)]
        stream: String,
        /// How many lines to tail.
        #[arg(long, default_value_t = 50)]
        n: i64,
    },
}
