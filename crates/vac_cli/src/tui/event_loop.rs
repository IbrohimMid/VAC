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
    let mut tg = TerminalGuard::new(true)?;

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
            // route_update handles Completed — but we also get it here for status sync
            let _ = prompt;
        }
        TaskEvent::Failed { prompt, error, status, history } => {
            app.status = status;
            app.session_mut().history = history;
            app.session_mut().active_task = None;
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
            if app.detail.is_some() {
                app.detail = DetailMode::None;
                app.focus = FocusPane::History;
            }
        }
        KeyCode::Tab => {
            if app.input.is_empty() { app.cycle_focus(); } else { app.show_help = !app.show_help; }
        }
        KeyCode::Up => {
            if app.focus == FocusPane::History { app.history_up(); } else { app.scroll_up(); }
        }
        KeyCode::Down => {
            if app.focus == FocusPane::History { app.history_down(); } else { app.scroll_down(); }
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
                    app.input.clear();
                    spawn_task(prompt, engine, tx, project_root);
                }
            }
        }
        KeyCode::Backspace => { app.input.pop(); }
        KeyCode::Char(ch) => {
            if app.focus != FocusPane::Composer { app.focus = FocusPane::Composer; }
            app.input.push(ch);
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
        let result = eng.run_task_with_updates(&prompt, Some(updates_tx)).await;
        let status = eng.status().await.unwrap_or_else(|_| fallback_status(project_root.clone()));
        let history = eng.history().await.unwrap_or_default();

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

struct TuiPolicyEngine {
    sender: Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<TaskEvent>>>>,
    allow: HashMap<String, bool>,
}

impl TuiPolicyEngine {
    fn new(sender: mpsc::UnboundedSender<TaskEvent>) -> Self {
        let allow = ["file_read","glob","grep","search","task_done","todo_write","vil_knowledge","vil_diagnostics","vil_lsp_query","vil_status"]
            .iter().map(|n| (n.to_string(), true)).collect();
        Self { sender: Arc::new(std::sync::Mutex::new(Some(sender))), allow }
    }
    fn set_sender(&self, s: mpsc::UnboundedSender<TaskEvent>) {
        if let Ok(mut g) = self.sender.lock() { *g = Some(s); }
    }
    fn needs_approval(name: &str) -> bool {
        matches!(name, "file_write" | "file_edit" | "bash" | "git" | "cargo")
    }
}

#[async_trait]
impl PolicyEngine for TuiPolicyEngine {
    async fn decide(&self, tool_name: &str, args: &serde_json::Value, context: &ToolContext) -> PolicyDecision {
        use vac_tools::registry::AgentZone;
        if context.agent_zone == AgentZone::SandboxedSubagent {
            if Self::needs_approval(tool_name) {
                return PolicyDecision::Deny(format!("{tool_name} denied in sandbox"));
            }
        }
        if self.allow.get(tool_name).copied().unwrap_or(false) { return PolicyDecision::Allow; }
        if !Self::needs_approval(tool_name) { return PolicyDecision::Allow; }

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
    }
}
