//! VAC CLI — Vastar Agentic CLI entry point.

mod commands;
mod io;
mod output;
mod telemetry;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

const VAC_AFTER_HELP: &str = "\
Command groups:
  Run:             run, interactive, autopilot, resume
  Config:          config, auth, rulebook, isolation, migrate, doctor
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
    },
    /// Interactive REPL mode
    #[command(next_help_heading = "Run")]
    Interactive {
        #[arg(long)]
        resume: bool,
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
    /// Resume from checkpoint
    #[command(next_help_heading = "Run")]
    Resume { checkpoint: PathBuf },

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
    /// Restore file to pre-agent state from snapshot journal
    #[command(next_help_heading = "VIL Tooling")]
    Restore {
        /// File path to restore (relative to project root)
        file: PathBuf,
    },
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
    #[command(hide = true)]
    Run,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let interactive_mode = matches!(&cli.command, Commands::Interactive { .. });

    if !interactive_mode {
        telemetry::init(
            cli.verbose,
            &cli.log_format,
            cli.otel_endpoint.as_deref(),
            cli.metrics_addr.as_deref(),
        )?;
    }

    let project_root = cli.project.unwrap_or_else(|| {
        #[allow(clippy::expect_used)]
        std::env::current_dir().expect("Failed to get current directory")
    });

    match cli.command {
        Commands::Doctor {
            strict,
            fix,
            interactive,
        } => commands::doctor::execute(project_root, &cli.format, strict, fix, interactive).await?,
        Commands::Init { force } => commands::init::execute(project_root, force).await?,
        Commands::Run {
            task,
            priority,
            profile,
            approve,
            target,
        } => {
            commands::run::execute(project_root, task, priority, profile, approve, target).await?;
        }
        Commands::Interactive {
            resume,
            record,
            replay,
        } => commands::interactive::execute(project_root, resume, record, replay).await?,
        Commands::Resume { checkpoint } => {
            commands::resume::execute(project_root, checkpoint).await?
        }
        Commands::Restore { file } => commands::restore::execute(project_root, file).await?,
        Commands::Status => commands::status::execute(project_root, &cli.format).await?,
        Commands::Signal(cmd) => commands::signal::dispatch(project_root, &cli.format, cmd).await?,
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
            commands::trajectory::why(project_root, &cli.format, path, trajectory).await?
        }
        Commands::Config { action } => commands::config::execute(project_root, action).await?,
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
            RulebookAction::List => commands::rulebook::execute_list(project_root).await?,
            RulebookAction::Validate => commands::rulebook::execute_validate(project_root).await?,
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
            RuntimeAction::Start => commands::runtime::execute_start(project_root).await?,
            RuntimeAction::Cancel { id } => {
                commands::runtime::execute_cancel(project_root, id).await?
            }
            RuntimeAction::Retry { id } => {
                commands::runtime::execute_retry(project_root, id).await?
            }
            RuntimeAction::Inspect { id } => {
                commands::runtime::execute_inspect(project_root, id, &cli.format).await?
            }
        },
        Commands::Isolation { action } => match action {
            IsolationAction::Status => {
                commands::isolation::execute_status(project_root, &cli.format).await?
            }
            IsolationAction::Logs => commands::isolation::execute_logs(project_root).await?,
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
        Commands::Mcp { action } => match action {
            McpAction::List => commands::mcp::list(&project_root)?,
            McpAction::Status => commands::mcp::status(&project_root).await?,
        },
        Commands::Autopilot { action } => match action {
            AutopilotAction::Up { execute } => {
                commands::autopilot::execute_up(project_root, execute).await?
            }
            AutopilotAction::Down => commands::autopilot::execute_down(project_root).await?,
            AutopilotAction::Status => {
                commands::autopilot::execute_status(project_root, &cli.format).await?
            }
            AutopilotAction::Run => commands::autopilot::execute_run(project_root).await?,
        },
        Commands::Migrate => {
            commands::migrate::execute(project_root).await?;
        }
    }

    Ok(())
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
