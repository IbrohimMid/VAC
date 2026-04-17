//! VAC CLI — Vastar Agentic CLI entry point.

mod commands;
mod output;
mod telemetry;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vac",
    version,
    about = "VAC — Vastar Agentic CLI: Autonomous development powered by VIL Engine",
    long_about = None,
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
    /// Check VAC subsystem readiness
    Doctor {
        #[arg(long)]
        strict: bool,
        #[arg(long)]
        fix: bool,
        #[arg(long, short)]
        interactive: bool,
    },
    /// Initialize VIL project context
    Init {
        #[arg(short, long)]
        force: bool,
    },
    /// Execute a single task via agent swarm
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
    Interactive {
        #[arg(long)]
        resume: bool,
    },
    /// Resume from checkpoint
    Resume { checkpoint: PathBuf },
    /// Restore file to pre-agent state from snapshot journal
    Restore {
        /// File path to restore (relative to project root)
        file: PathBuf,
    },
    /// Show engine status
    Status,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage authentication
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Export session artifact
    Export {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long, default_value = "vac-cbor")]
        format: String,
        #[arg(long)]
        sign: bool,
    },
    /// Import session bundle into current project
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
    /// Manage VIL-native rulebooks (SOP, playbooks, governance constraints)
    Rulebook {
        #[command(subcommand)]
        action: RulebookAction,
    },
    /// Start ACP editor-facing agent server
    Acp {
        #[arg(long, default_value = "4123")]
        port: u16,
    },
    /// Background runtime management
    Runtime {
        #[command(subcommand)]
        action: RuntimeAction,
    },
    /// Execution boundary and isolation management
    Isolation {
        #[command(subcommand)]
        action: IsolationAction,
    },
    /// Model Context Protocol (MCP) server management
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Autopilot daemon — 24/7 autonomous runtime with VIL policy enforcement
    Autopilot {
        #[command(subcommand)]
        action: AutopilotAction,
    },
    /// Migrate .vac/ schema to the latest version
    Migrate,
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
    /// Start autopilot daemon (spawns background runtime, writes PID + event log)
    Up,
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

    let project_root = cli
        .project
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

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
        Commands::Interactive { resume } => {
            commands::interactive::execute(project_root, resume).await?
        }
        Commands::Resume { checkpoint } => {
            commands::resume::execute(project_root, checkpoint).await?
        }
        Commands::Restore { file } => commands::restore::execute(project_root, file).await?,
        Commands::Status => commands::status::execute(project_root, &cli.format).await?,
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
            AutopilotAction::Up => commands::autopilot::execute_up(project_root).await?,
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
