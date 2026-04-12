//! VAC CLI — Vastar Agentic CLI entry point.

mod commands;
mod output;
mod tui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(
    name = "vac",
    version,
    about = "VAC — Vastar Agentic CLI: Autonomous development powered by VIL Engine",
    long_about = None,
)]
struct Cli {
    /// Path to project root (defaults to current directory)
    #[arg(short = 'C', long, global = true)]
    project: Option<PathBuf>,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Output format: "text", "json"
    #[arg(long, default_value = "text", global = true)]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize VIL project context, scan codebase, build IR index
    Init {
        /// Force re-initialization even if already initialized
        #[arg(short, long)]
        force: bool,
    },

    /// Execute a single task via agent swarm
    Run {
        /// Task description
        task: String,

        /// Priority: low, normal, high, critical
        #[arg(short, long, default_value = "normal")]
        priority: String,

        /// Require approval before applying changes
        #[arg(long)]
        approve: bool,

        /// Restrict to specific files/directories
        #[arg(short, long)]
        target: Vec<String>,
    },

    /// Interactive REPL mode with streaming output
    Interactive {
        /// Resume previous session
        #[arg(long)]
        resume: bool,
    },

    /// Show active agents, memory usage, task progress
    Status,

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage VAC authentication for Kilo Gateway
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Export session as VAC artifact
    Export {
        /// Output path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Export format: vac-cbor, opencode-json, claude-jsonl
        #[arg(short, long, default_value = "vac-cbor")]
        format: String,

        /// Enable COSE signing
        #[arg(long)]
        sign: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set { key: String, value: String },
    /// Add an LLM provider
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
    /// Save a Kilo Gateway token for future VAC sessions
    Login {
        /// Token value to save. If omitted, VAC will prompt on stdin.
        #[arg(long)]
        token: Option<String>,
    },
    /// Show whether VAC can authenticate
    Status,
    /// Remove saved VAC authentication
    Logout,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let interactive_mode = matches!(&cli.command, Commands::Interactive { .. });

    // Keep alternate-screen TUI clean by disabling terminal log output in interactive mode.
    if !interactive_mode {
        let filter = match cli.verbose {
            0 => "warn,vac=info",
            1 => "info,vac=debug",
            2 => "debug",
            _ => "trace",
        };
        fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
            .with_target(false)
            .init();
    }

    let project_root = cli
        .project
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    match cli.command {
        Commands::Init { force } => {
            commands::init::execute(project_root, force).await?;
        }
        Commands::Run {
            task,
            priority,
            approve,
            target,
        } => {
            commands::run::execute(project_root, task, priority, approve, target).await?;
        }
        Commands::Interactive { resume } => {
            commands::interactive::execute(project_root, resume).await?;
        }
        Commands::Status => {
            commands::status::execute(project_root).await?;
        }
        Commands::Config { action } => {
            commands::config::execute(project_root, action).await?;
        }
        Commands::Auth { action } => {
            commands::auth::execute(action).await?;
        }
        Commands::Export {
            output,
            format,
            sign,
        } => {
            commands::export::execute(project_root, output, format, sign).await?;
        }
    }

    Ok(())
}
