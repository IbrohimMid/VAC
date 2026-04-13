//! Event loop — input thread, redraw, runtime bridge.
//! Loop pattern adapted from stakpak/tui/src/event_loop.rs (Apache-2.0).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use vac_core::engine::RuntimeUpdate;
use vac_core::{TaskResult, VacEngine};
use vac_runtime::executor::{OperatingMode, TaskExecutor};
use vac_runtime::queue::TaskQueue;
use vac_tools::registry::ToolContext;
use vac_tools::router::{PolicyDecision, PolicyEngine};

use super::app::{FocusPane, PendingApproval, TuiApp};
use super::services::detail::DetailMode;
use super::services::mouse::handle_mouse;
use super::services::telemetry::route_update;
use super::terminal::TerminalGuard;
use super::view::render;

// ── Task events ───────────────────────────────────────────────────────────────

pub enum TaskEvent {
    Started { prompt: String },
    Update(RuntimeUpdate),
    Finished {
        prompt: String,
        result: TaskResult,
        status: vac_core::engine::EngineStatus,
        history: Vec<vac_core::engine::TaskHistoryEntry>,
    },
    Failed {
        prompt: String,
        error: String,
        status: vac_core::engine::EngineStatus,
        history: Vec<vac_core::engine::TaskHistoryEntry>,
    },
    ApprovalRequest {
        tool_name: String,
        args_preview: String,
        responder: oneshot::Sender<bool>,
    },
    /// Periodic snapshot of the runtime job queue (optional integration).
    RuntimeJobsUpdate {
        jobs: Vec<vac_runtime::jobs::Job>,
        mode: vac_runtime::executor::OperatingMode,
    },
    /// Session restored successfully.
    SessionRestored {
        session_id: uuid::Uuid,
        session_title: String,
        checkpoint: vil_swarm::checkpoint::CheckpointEnvelope,
        status: vac_core::engine::EngineStatus,
        history: Vec<vac_core::engine::TaskHistoryEntry>,
    },
    SessionRestoreFailed {
        error: String,
    },
}

// ── Main loop ─────────────────────────────────────────────────────────────────

pub async fn run(project_root: PathBuf, _resume: bool) -> anyhow::Result<()> {
    let (event_tx_placeholder, _) = mpsc::unbounded_channel::<TaskEvent>();
    let policy = Arc::new(TuiPolicyEngine::new(event_tx_placeholder));

    let mut engine = VacEngine::new(project_root.clone()).await?;
    engine.init_with_policy(Some(policy.clone())).await?;
    let status = engine.status().await?;
    let history = engine.history().await.unwrap_or_default();

    let auth_hint = missing_auth_hint();
    let (tx, mut rx) = mpsc::unbounded_channel::<TaskEvent>();
    policy.set_sender(tx.clone());

    let engine = Arc::new(Mutex::new(engine));
    let mut app = TuiApp::new(status, history, auth_hint);

    // Restore TUI state from session metadata
    {
        let eng = engine.lock().await;
        let session = eng.session().read().await;
        app.restore_tui_state(&session.metadata);
    }

    let mut tg = TerminalGuard::new(true).map_err(|e| {
        anyhow::anyhow!("Failed to initialize terminal (TTY required): {}", e)
    })?;

    // Optional runtime bridge — spawn background poller if runtime is available
    spawn_runtime_bridge(tx.clone(), project_root.clone());

    while app.running {
        app.spinner_tick = app.spinner_tick.wrapping_add(1);

        // Drain all pending events (non-blocking batch)
        while let Ok(ev) = rx.try_recv() {
            handle_task_event(&mut app, ev);
        }

        tg.terminal.draw(|f| render(f, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key(key, &mut app, engine.clone(), tx.clone(), project_root.clone())? {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse(mouse, &mut app);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    // Graceful exit: save TUI state to session
    {
        let mut eng = engine.lock().await;
        app.persist_session_state(&mut eng).await;
    }

    Ok(())
}

// ── Task event handler ────────────────────────────────────────────────────────

fn handle_task_event(app: &mut TuiApp, ev: TaskEvent) {
    match ev {
        TaskEvent::Started { prompt } => {
            app.session_mut().active_task = Some(prompt.clone());
            app.push_transcript("You", prompt.clone(), ratatui::style::Color::Yellow);
            app.push_thinking(format!("Task: {}", &prompt[..prompt.len().min(56)]));
            app.set_activity("Thinking", format!("Task: {}", &prompt[..prompt.len().min(56)]));
            app.show_help = false;
            app.live_diff_files.clear();
            app.detail = DetailMode::None;
            app.scroll.transcript.pin_to_bottom();
            app.focus = FocusPane::Transcript;
        }
        TaskEvent::Update(update) => {
            route_update(app, update);
        }
        TaskEvent::Finished { prompt, result, status, history } => {
            app.status = status;
            app.session_mut().history = history;
            app.session_mut().active_task = None;
            app.cancel_token = None;
            // route_update handles Completed — but we also get it here for status sync
            let _ = prompt;
        }
        TaskEvent::Failed { prompt, error, status, history } => {
            app.status = status;
            app.session_mut().history = history;
            app.session_mut().active_task = None;
            app.cancel_token = None;
            let _ = prompt;
            // route_update handles Failed — but ensure auth hint
            if let Some(hint) = auth_hint_for_error(&error) {
                app.push_transcript("Hint", hint.clone(), ratatui::style::Color::Yellow);
                app.push_commands(hint[..hint.len().min(48)].to_string());
            }
        }
        TaskEvent::ApprovalRequest { tool_name, args_preview, responder } => {
            let summary = format!("{tool_name}: {}", &args_preview[..args_preview.len().min(60)]);
            app.pending_approval = Some(PendingApproval {
                tool_name,
                summary: summary.clone(),
                args_preview,
                responder,
            });
            app.push_commands(format!("Approval needed: {}", &summary[..summary.len().min(46)]));
            app.set_activity("Awaiting approval", summary);
        }
        TaskEvent::RuntimeJobsUpdate { jobs, mode } => {
            app.runtime_jobs = jobs;
            app.operating_mode = Some(mode);
        }
        TaskEvent::SessionRestored { session_id, session_title, checkpoint, status, history } => {
            app.status = status.clone();
            app.session_mut().history = history.clone();

            // Clear and restore transcript from checkpoint
            let session_tab = app.session_mut();
            session_tab.transcript.clear();
            session_tab.transcript.push(super::app::TranscriptEntry {
                label: "System".to_string(),
                body: format!("Restored: {}", session_title),
                color: ratatui::style::Color::Cyan,
            });

            for msg in &checkpoint.messages {
                let label = format!("{:?}", msg.role);
                let color = if label.contains("User") {
                    ratatui::style::Color::Green
                } else if label.contains("Assistant") {
                    ratatui::style::Color::Blue
                } else {
                    ratatui::style::Color::Gray
                };

                session_tab.transcript.push(super::app::TranscriptEntry {
                    label,
                    body: msg.content.clone(),
                    color,
                });
            }

            app.push_transcript("System", format!("✓ Restored {} messages", checkpoint.messages.len()), ratatui::style::Color::Green);
        }
        TaskEvent::SessionRestoreFailed { error } => {
            app.push_transcript("Error", format!("Failed to restore session: {}", error), ratatui::style::Color::Red);
        }
    }
}

fn handle_key(
    key: KeyEvent,
    app: &mut TuiApp,
    engine: Arc<Mutex<VacEngine>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
    project_root: PathBuf,
) -> anyhow::Result<bool> {
    // Approval modal intercepts all keys
    if let Some(pending) = app.pending_approval.take() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = pending.responder.send(true);
                app.push_commands(format!("Approved {}", pending.tool_name));
                app.push_transcript("Approval", format!("Allowed {}", pending.summary), ratatui::style::Color::Green);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                let _ = pending.responder.send(false);
                app.push_commands(format!("Denied {}", pending.tool_name));
                app.push_transcript("Approval", format!("Denied {}", pending.summary), ratatui::style::Color::Red);
            }
            _ => { app.pending_approval = Some(pending); }
        }
        return Ok(false);
    }

    // Sessions popup intercepts keys
    if app.show_sessions_popup {
        match key.code {
            KeyCode::Esc => {
                app.show_sessions_popup = false;
                app.session_search.clear();
            }
            KeyCode::Up => {
                if app.session_selected > 0 {
                    app.session_selected -= 1;
                }
            }
            KeyCode::Down => {
                let filtered_count = app.available_sessions
                    .iter()
                    .filter(|session| {
                        if app.session_search.is_empty() {
                            true
                        } else {
                            session.title.to_lowercase().contains(&app.session_search.to_lowercase())
                        }
                    })
                    .count();
                
                if app.session_selected + 1 < filtered_count {
                    app.session_selected += 1;
                }
            }
            KeyCode::Enter => {
                // Filter sessions to find the actual selected one
                let filtered: Vec<_> = app.available_sessions
                    .iter()
                    .filter(|session| {
                        if app.session_search.is_empty() {
                            true
                        } else {
                            session.title.to_lowercase().contains(&app.session_search.to_lowercase())
                        }
                    })
                    .collect();

                if let Some(session) = filtered.get(app.session_selected) {
                    let session_uuid = session.session_id;
                    let session_title = session.title.clone();
                    let checkpoint_path = session.checkpoint_path.clone();

                    let session_tab = app.session_mut();
                    session_tab.transcript.clear();
                    session_tab.transcript.push(super::app::TranscriptEntry {
                        label: "System".to_string(),
                        body: format!("Restoring session: {}...", session_title),
                        color: ratatui::style::Color::Cyan,
                    });

                    spawn_session_restore(
                        session_uuid,
                        session_title,
                        checkpoint_path,
                        engine.clone(),
                        tx.clone(),
                    );
                }
                app.show_sessions_popup = false;
                app.session_search.clear();
            }
            KeyCode::Char(ch) => {
                app.session_search.push(ch);
                // Filter sessions by search
                app.session_selected = 0; // Reset selection when search changes
            }
            KeyCode::Backspace => {
                app.session_search.pop();
                // Re-filter sessions
                app.session_selected = 0; // Reset selection when search changes
            }
            _ => {}
        }
        return Ok(false);
    }

    // Revert confirm modal
    if let DetailMode::RevertConfirm(idx) = app.detail.clone() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.history.list.select(Some(idx));
                app.revert_selected(&project_root);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.detail = DetailMode::None;
                // Return focus to history, not composer
                app.focus = FocusPane::History;
            }
            _ => {} // Ignore other keys, keep modal open
        }
        return Ok(false);
    }

    // Detail panel: R for revert, Esc to close
    if app.focus == FocusPane::Detail {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if let DetailMode::TaskDetail(idx) = app.detail.clone() {
                    app.detail = DetailMode::RevertConfirm(idx);
                }
                return Ok(false);
            }
            KeyCode::Esc => {
                app.detail = DetailMode::None;
                app.focus = FocusPane::History;
                return Ok(false);
            }
            _ => {}
        }
    }

    // Ctrl combos
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => { app.running = false; return Ok(true); }
            KeyCode::Char('n') => { app.new_session(); return Ok(false); }
            KeyCode::Char(']') => { app.next_session(); return Ok(false); }
            KeyCode::Char('[') => { app.prev_session(); return Ok(false); }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('q') if app.input.is_empty() && app.session().active_task.is_none() => {
            app.running = false;
            return Ok(true);
        }
        KeyCode::Esc => {
            app.input.clear();
            // Cancel running task if Esc pressed while busy
            if app.session().active_task.is_some() {
                if let Some(token) = app.cancel_token.take() {
                    token.cancel();
                }
            }
            if app.detail.is_some() {
                app.detail = DetailMode::None;
                app.focus = FocusPane::History;
            }
        }
        KeyCode::Tab => {
            if app.show_command_list {
                // Autocomplete selected command
                let commands = super::app::TuiApp::available_commands();
                if let Some(cmd) = commands.get(app.command_selected) {
                    app.input = cmd.name.clone();
                    app.show_command_list = false;
                }
            } else if app.input.is_empty() { 
                app.cycle_focus(); 
            } else { 
                app.show_help = !app.show_help; 
            }
        }
        KeyCode::Up => {
            if app.show_command_list {
                if app.command_selected > 0 {
                    app.command_selected -= 1;
                }
            } else if app.focus == FocusPane::History { 
                app.history_up(); 
            } else { 
                app.scroll_up(); 
            }
        }
        KeyCode::Down => {
            if app.show_command_list {
                let max = super::app::TuiApp::available_commands().len().saturating_sub(1);
                if app.command_selected < max {
                    app.command_selected += 1;
                }
            } else if app.focus == FocusPane::History { 
                app.history_down(); 
            } else { 
                app.scroll_down(); 
            }
        }
        KeyCode::PageUp => { for _ in 0..5 { app.scroll_up(); } }
        KeyCode::PageDown => { for _ in 0..5 { app.scroll_down(); } }
        KeyCode::Char('r') | KeyCode::Char('R') if app.input.is_empty() && app.focus == FocusPane::History => {
            if let Some(idx) = app.history.selected() {
                app.detail = DetailMode::RevertConfirm(idx);
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') if app.input.is_empty() && app.focus == FocusPane::History => {
            if let Some(idx) = app.history.selected() {
                app.detail = DetailMode::TaskDetail(idx);
                app.focus = FocusPane::Detail;
            }
        }
        KeyCode::Enter => {
            if app.focus == FocusPane::History {
                if let Some(idx) = app.history.selected() {
                    app.detail = DetailMode::TaskDetail(idx);
                    app.focus = FocusPane::Detail;
                }
                return Ok(false);
            }
            if app.session().active_task.is_none() {
                let prompt = app.input.trim().to_string();
                if !prompt.is_empty() {
                    // Handle slash commands
                    if prompt == "/sessions" {
                        app.show_sessions_popup = true;
                        app.session_selected = 0;
                        app.session_search.clear();
                        // Load sessions from checkpoint directory
                        let checkpoint_dir = project_root.join(".vac").join("checkpoints");
                        app.available_sessions = vil_swarm::checkpoint::list_sessions(&checkpoint_dir);
                        if app.available_sessions.is_empty() {
                            app.push_transcript("System", "No saved sessions found".to_string(), ratatui::style::Color::Yellow);
                            app.show_sessions_popup = false;
                        }
                        app.input.clear();
                        return Ok(false);
                    } else if prompt == "/resume" {
                        // Resume last session
                        let checkpoint_dir = project_root.join(".vac").join("checkpoints");
                        let sessions = vil_swarm::checkpoint::list_sessions(&checkpoint_dir);

                        if let Some(last_session) = sessions.first() {
                            let session_uuid = last_session.session_id;
                            let session_title = last_session.title.clone();
                            let checkpoint_path = last_session.checkpoint_path.clone();

                            let session_tab = app.session_mut();
                            session_tab.transcript.clear();
                            session_tab.transcript.push(super::app::TranscriptEntry {
                                label: "System".to_string(),
                                body: format!("Restoring session: {}...", session_title),
                                color: ratatui::style::Color::Cyan,
                            });

                            spawn_session_restore(
                                session_uuid,
                                session_title,
                                checkpoint_path,
                                engine.clone(),
                                tx.clone(),
                            );
                        } else {
                            app.push_transcript("System", "No saved sessions found".to_string(), ratatui::style::Color::Yellow);
                        }
                        app.input.clear();
                        return Ok(false);
                    }
                    
                    app.input.clear();
                    let cancel = tokio_util::sync::CancellationToken::new();
                    app.cancel_token = Some(cancel.clone());
                    spawn_task(prompt, engine, tx, project_root, cancel);
                }
            }
        }
        KeyCode::Backspace => { 
            app.input.pop(); 
            // Hide command list if input is cleared
            if app.input.is_empty() {
                app.show_command_list = false;
            }
        }
        KeyCode::Char(ch) => {
            if app.focus != FocusPane::Composer { app.focus = FocusPane::Composer; }
            app.input.push(ch);
            // Show command list when user types /
            if app.input == "/" {
                app.show_command_list = true;
                app.command_selected = 0;
            } else if !app.input.starts_with('/') {
                app.show_command_list = false;
            }
        }
        _ => {}
    }
    Ok(false)
}

// ── Task spawner ──────────────────────────────────────────────────────────────

fn spawn_task(
    prompt: String,
    engine: Arc<Mutex<VacEngine>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
    project_root: PathBuf,
    cancel: tokio_util::sync::CancellationToken,
) {
    let _ = tx.send(TaskEvent::Started { prompt: prompt.clone() });
    tokio::spawn(async move {
        let (updates_tx, mut updates_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
        let fwd = tx.clone();
        tokio::spawn(async move {
            while let Some(u) = updates_rx.recv().await {
                let _ = fwd.send(TaskEvent::Update(u));
            }
        });

        let mut eng = engine.lock().await;
        let result = eng.run_task_with_cancel(&prompt, Some(updates_tx), Some(cancel.clone())).await;
        let status = eng.status().await.unwrap_or_else(|_| fallback_status(project_root.clone()));
        let history = eng.history().await.unwrap_or_default();
        let _ = cancel; // token kept alive until task completes

        match result {
            Ok(r) => { let _ = tx.send(TaskEvent::Finished { prompt, result: r, status, history }); }
            Err(e) => { let _ = tx.send(TaskEvent::Failed { prompt, error: e.to_string(), status, history }); }
        }
    });
}

fn fallback_status(project_root: PathBuf) -> vac_core::engine::EngineStatus {
    vac_core::engine::EngineStatus {
        project_root,
        session_id: uuid::Uuid::nil(),
        total_tasks: 0,
        completed_tasks: 0,
        failed_tasks: 0,
        total_tokens_used: 0,
        subsystems_initialized: true,
    }
}

// ── Session restore spawner ──────────────────────────────────────────────────

fn spawn_session_restore(
    session_id: uuid::Uuid,
    session_title: String,
    checkpoint_path: std::path::PathBuf,
    engine: Arc<Mutex<VacEngine>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
) {
    tokio::spawn(async move {
        let mut eng = engine.lock().await;

        // Load checkpoint for transcript using canonical path from SessionInfo
        let checkpoint = vil_swarm::checkpoint::load_checkpoint_from_file(&checkpoint_path);

        match eng.load_session(session_id).await {
            Ok(()) => {
                let project_root = eng
                    .status()
                    .await
                    .map(|s| s.project_root.clone())
                    .unwrap_or_default();
                let status = eng
                    .status()
                    .await
                    .unwrap_or_else(|_| fallback_status(project_root));
                let history = eng.history().await.unwrap_or_default();

                match checkpoint {
                    Ok(cp) => {
                        let _ = tx.send(TaskEvent::SessionRestored {
                            session_id,
                            session_title,
                            checkpoint: cp,
                            status,
                            history,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(TaskEvent::SessionRestoreFailed {
                            error: format!(
                                "Session loaded but checkpoint missing/unreadable: {}",
                                e
                            ),
                        });
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(TaskEvent::SessionRestoreFailed {
                    error: e.to_string(),
                });
            }
        }
    });
}

// ── Runtime bridge ────────────────────────────────────────────────────────────

/// Spawn a background task that polls the runtime job queue every 2s and
/// forwards snapshots to the TUI event loop. Non-fatal — errors are silently
/// ignored so the existing `vac interactive` flow is never broken.
fn spawn_runtime_bridge(tx: mpsc::UnboundedSender<TaskEvent>, project_root: PathBuf) {
    let mode_str = std::env::var("VAC_OPERATING_MODE").unwrap_or_default();
    let mode = OperatingMode::from_str(&mode_str);
    let queue = Arc::new(TaskQueue::new());
    let _executor = Arc::new(TaskExecutor::new(project_root, mode.clone()));

    tokio::spawn(async move {
        loop {
            let jobs = queue.list().await;
            if tx.send(TaskEvent::RuntimeJobsUpdate { jobs, mode: mode.clone() }).is_err() {
                break; // TUI has exited
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

fn missing_auth_hint() -> Option<String> {
    match vac_core::auth::auth_status() {
        Ok(s) if s.effective_present => None,
        _ => Some("Not authenticated. Run `vac auth login` then restart.".to_string()),
    }
}

fn auth_hint_for_error(error: &str) -> Option<String> {
    if error.contains("API key") || error.contains("api_key") {
        missing_auth_hint()
    } else {
        None
    }
}

// ── Policy engine ─────────────────────────────────────────────────────────────

use std::collections::HashMap;
use async_trait::async_trait;
use crate::tui::services::tool_policy;

struct TuiPolicyEngine {
    sender: Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<TaskEvent>>>>,
}

impl TuiPolicyEngine {
    fn new(sender: mpsc::UnboundedSender<TaskEvent>) -> Self {
        Self { sender: Arc::new(std::sync::Mutex::new(Some(sender))) }
    }
    fn set_sender(&self, s: mpsc::UnboundedSender<TaskEvent>) {
        if let Ok(mut g) = self.sender.lock() { *g = Some(s); }
    }
}

#[async_trait]
impl PolicyEngine for TuiPolicyEngine {
    async fn decide(&self, tool_name: &str, args: &serde_json::Value, context: &ToolContext) -> PolicyDecision {
        // TODO: Read registry metadata for risk_level override if available
        // For now, delegate to tool_policy classification
        
        if tool_policy::needs_approval(tool_name, context.agent_zone) {
            // Sandbox denies write/exec entirely
            use vac_tools::registry::AgentZone;
            if context.agent_zone == AgentZone::SandboxedSubagent {
                return PolicyDecision::Deny(format!("{tool_name} denied in sandbox"));
            }
            
            // Normal mode: prompt user for approval
            let preview = serde_json::to_string(args).unwrap_or_default();
            let (resp_tx, resp_rx) = oneshot::channel();
            let sender = self.sender.lock().ok().and_then(|g| g.as_ref().cloned());
            if let Some(s) = sender {
                let _ = s.send(TaskEvent::ApprovalRequest {
                    tool_name: tool_name.to_string(),
                    args_preview: preview,
                    responder: resp_tx,
                });
                match resp_rx.await {
                    Ok(true) => PolicyDecision::Allow,
                    Ok(false) => PolicyDecision::Deny(format!("User denied {tool_name}")),
                    Err(_) => PolicyDecision::Deny(format!("Approval channel closed")),
                }
            } else {
                PolicyDecision::Allow // no UI attached, auto-allow
            }
        } else {
            // Read-safe tools auto-allowed
            PolicyDecision::Allow
        }
    }
}
