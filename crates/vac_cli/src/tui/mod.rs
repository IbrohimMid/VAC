//! TUI module — interactive terminal UI for VAC.

pub mod lanes;

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use tokio::sync::{Mutex, mpsc, oneshot};

use vac_core::engine::{EngineStatus, RuntimeUpdate, TaskHistoryEntry};
use vac_core::{TaskResult, VacEngine};
use vac_tools::registry::ToolContext;
use vac_tools::router::{PolicyDecision, PolicyEngine};

#[derive(Clone)]
pub struct TranscriptEntry {
    pub label: String,
    pub body: String,
    pub color: Color,
}

struct PendingApproval {
    tool_name: String,
    summary: String,
    args_preview: String,
    responder: oneshot::Sender<bool>,
}

pub struct TuiApp {
    pub running: bool,
    pub input: String,
    pub status: EngineStatus,
    pub history: Vec<TaskHistoryEntry>,
    pub trigger_lane_log: Vec<String>,
    pub data_lane_log: Vec<String>,
    pub control_lane_log: Vec<String>,
    pub transcript: Vec<TranscriptEntry>,
    pub active_task: Option<String>,
    pub last_result: Option<TaskResult>,
    pub diff_preview: String,
    pub show_help: bool,
    pub active_provider: String,
    pub active_model: String,
    pub current_phase: String,
    pub last_activity: String,
    pub auth_ready: bool,
    pub auth_hint: Option<String>,
    spinner_tick: usize,
    pending_approval: Option<PendingApproval>,
    streaming_assistant: Option<usize>,
}

impl TuiApp {
    pub fn new(status: EngineStatus, history: Vec<TaskHistoryEntry>) -> Self {
        let mut transcript = Vec::new();
        transcript.push(TranscriptEntry {
            label: "VAC".to_string(),
            body: "OpenCode-style TUI ready. Type a task, press Enter, and approve write tools with y/n."
                .to_string(),
            color: Color::Cyan,
        });
        let auth_hint = missing_api_key_hint();
        let auth_ready = auth_hint.is_none();
        let mut control_lane_log = vec!["Waiting for task".to_string()];
        if let Some(hint) = &auth_hint {
            transcript.push(TranscriptEntry {
                label: "Auth".to_string(),
                body: hint.clone(),
                color: Color::Yellow,
            });
            control_lane_log.push("Run `vac auth login` before running tasks".to_string());
        }

        Self {
            running: true,
            input: String::new(),
            status,
            history,
            trigger_lane_log: vec!["TUI booted".to_string()],
            data_lane_log: vec!["No reads yet".to_string()],
            control_lane_log,
            transcript,
            active_task: None,
            last_result: None,
            diff_preview: "No file changes yet.".to_string(),
            show_help: true,
            active_provider: "kilo".to_string(),
            active_model: std::env::var("KILO_MODEL")
                .unwrap_or_else(|_| "kilo-auto/free".to_string()),
            current_phase: "Idle".to_string(),
            last_activity: "Waiting for task".to_string(),
            auth_ready,
            auth_hint,
            spinner_tick: 0,
            pending_approval: None,
            streaming_assistant: None,
        }
    }

    fn push_transcript(&mut self, label: impl Into<String>, body: impl Into<String>, color: Color) {
        self.streaming_assistant = None;
        self.transcript.push(TranscriptEntry {
            label: label.into(),
            body: body.into(),
            color,
        });
        self.trim_transcript();
    }

    fn ensure_streaming_assistant(&mut self) {
        if self.streaming_assistant.is_none() {
            self.transcript.push(TranscriptEntry {
                label: "Assistant".to_string(),
                body: String::new(),
                color: Color::Green,
            });
            self.streaming_assistant = Some(self.transcript.len() - 1);
            self.trim_transcript();
        }
    }

    fn append_assistant_chunk(&mut self, chunk: &str) {
        self.ensure_streaming_assistant();
        if let Some(index) = self.streaming_assistant {
            if let Some(entry) = self.transcript.get_mut(index) {
                entry.body.push_str(chunk);
            }
        }
    }

    fn finish_streaming(&mut self) {
        self.streaming_assistant = None;
    }

    fn trim_transcript(&mut self) {
        if self.transcript.len() > 240 {
            let overflow = self.transcript.len() - 240;
            self.transcript.drain(0..overflow);
            if let Some(index) = self.streaming_assistant {
                self.streaming_assistant = index.checked_sub(overflow);
            }
        }
    }

    fn push_lane(log: &mut Vec<String>, message: impl Into<String>) {
        log.push(message.into());
        if log.len() > 24 {
            let overflow = log.len() - 24;
            log.drain(0..overflow);
        }
    }

    fn set_activity(&mut self, phase: impl Into<String>, detail: impl Into<String>) {
        self.current_phase = phase.into();
        self.last_activity = detail.into();
    }
}

enum TaskEvent {
    Started { prompt: String },
    Update(RuntimeUpdate),
    Finished {
        prompt: String,
        result: TaskResult,
        status: EngineStatus,
        history: Vec<TaskHistoryEntry>,
        diff_preview: String,
    },
    Failed {
        prompt: String,
        error: String,
        status: EngineStatus,
        history: Vec<TaskHistoryEntry>,
    },
    ApprovalRequest {
        tool_name: String,
        args_preview: String,
        responder: oneshot::Sender<bool>,
    },
}

struct TerminalGuard {
    terminal: Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub async fn run(project_root: PathBuf, _resume: bool) -> anyhow::Result<()> {
    let event_tx_placeholder = mpsc::unbounded_channel::<TaskEvent>().0;
    let policy = Arc::new(TuiPolicyEngine::new(event_tx_placeholder));

    let mut engine = VacEngine::new(project_root.clone()).await?;
    engine.init_with_policy(Some(policy.clone())).await?;
    let status = engine.status().await?;
    let history = engine.history().await.unwrap_or_default();

    let (tx, mut rx) = mpsc::unbounded_channel::<TaskEvent>();
    policy.set_sender(tx.clone());

    let engine = Arc::new(Mutex::new(engine));
    let mut app = TuiApp::new(status, history);
    let mut terminal = TerminalGuard::new()?;

    while app.running {
        app.spinner_tick = app.spinner_tick.wrapping_add(1);
        while let Ok(event) = rx.try_recv() {
            match event {
                TaskEvent::Started { prompt } => {
                    app.active_task = Some(prompt.clone());
                    app.push_transcript("You", prompt.clone(), Color::Yellow);
                    TuiApp::push_lane(&mut app.trigger_lane_log, format!("Queued: {}", truncate(&prompt, 56)));
                    TuiApp::push_lane(&mut app.control_lane_log, "Waiting for model response");
                    app.set_activity("Queued", format!("Task queued: {}", truncate(&prompt, 60)));
                    app.show_help = false;
                }
                TaskEvent::Update(update) => match update {
                    RuntimeUpdate::Status(message) => {
                        let human = humanize_status(&message);
                        TuiApp::push_lane(&mut app.trigger_lane_log, human.clone());
                        app.set_activity(human.clone(), human);
                    }
                    RuntimeUpdate::ModelInfo { provider, model } => {
                        app.active_provider = provider;
                        app.active_model = model.clone();
                        TuiApp::push_lane(
                            &mut app.trigger_lane_log,
                            format!("Model: {}", truncate(&model, 28)),
                        );
                        app.set_activity("Model ready", format!("Connected to {}", truncate(&model, 32)));
                    }
                    RuntimeUpdate::AssistantChunk(chunk) => {
                        app.append_assistant_chunk(&chunk);
                    }
                    RuntimeUpdate::ToolCall { name, arguments, .. } => {
                        app.finish_streaming();
                        let action = describe_tool_call(&name, &arguments);
                        let (label, color) = transcript_style_for_tool(&name);
                        app.push_transcript(
                            label,
                            action.clone(),
                            color,
                        );
                        if is_read_tool(&name) {
                            TuiApp::push_lane(&mut app.data_lane_log, action.clone());
                            app.set_activity("Reading", action);
                        } else {
                            TuiApp::push_lane(&mut app.control_lane_log, action.clone());
                            app.set_activity(activity_phase_for_tool(&name), action);
                        }
                    }
                    RuntimeUpdate::ToolResult {
                        name,
                        content,
                        success,
                        ..
                    } => {
                        app.finish_streaming();
                        let summary = summarize_tool_result(&name, &content, success);
                        let color = if success { Color::Blue } else { Color::Red };
                        app.push_transcript(
                            "Result",
                            summary.clone(),
                            color,
                        );
                        if is_read_tool(&name) {
                            TuiApp::push_lane(&mut app.data_lane_log, summary.clone());
                        } else {
                            TuiApp::push_lane(&mut app.control_lane_log, summary.clone());
                        }
                        let phase = if success { "Reviewing result" } else { "Tool failed" };
                        app.set_activity(phase, summary);
                    }
                    RuntimeUpdate::Completed(result) => {
                        app.last_result = Some(result);
                        app.finish_streaming();
                    }
                },
                TaskEvent::Finished {
                    prompt,
                    result,
                    status,
                    history,
                    diff_preview,
                } => {
                    app.status = status;
                    app.history = history;
                    app.active_task = None;
                    app.last_result = Some(result.clone());
                    app.diff_preview = diff_preview;
                    app.finish_streaming();
                    app.push_transcript("Summary", result.summary.clone(), Color::LightGreen);
                    app.set_activity("Idle", "Task finished");
                    TuiApp::push_lane(
                        &mut app.trigger_lane_log,
                        format!("Completed: {}", truncate(&prompt, 56)),
                    );
                    TuiApp::push_lane(
                        &mut app.data_lane_log,
                        format!("Tokens: {}", result.total_tokens_used),
                    );
                    TuiApp::push_lane(
                        &mut app.control_lane_log,
                        format!("Final status: {:?}", result.status),
                    );
                }
                TaskEvent::Failed {
                    prompt,
                    error,
                    status,
                    history,
                } => {
                    app.status = status;
                    app.history = history;
                    app.active_task = None;
                    app.finish_streaming();
                    app.push_transcript("Error", error.clone(), Color::Red);
                    app.set_activity("Failed", truncate(&error, 80));
                    if let Some(hint) = auth_hint_for_error(&error) {
                        app.push_transcript("Hint", hint.clone(), Color::Yellow);
                        TuiApp::push_lane(&mut app.control_lane_log, truncate(&hint, 48));
                    }
                    TuiApp::push_lane(
                        &mut app.trigger_lane_log,
                        format!("Failed: {}", truncate(&prompt, 56)),
                    );
                    TuiApp::push_lane(&mut app.control_lane_log, truncate(&error, 48));
                }
                TaskEvent::ApprovalRequest {
                    tool_name,
                    args_preview,
                    responder,
                } => {
                    let summary = describe_tool_call_from_preview(&tool_name, &args_preview);
                    app.pending_approval = Some(PendingApproval {
                        tool_name,
                        summary: summary.clone(),
                        args_preview,
                        responder,
                    });
                    TuiApp::push_lane(&mut app.control_lane_log, format!("Approval needed: {}", truncate(&summary, 46)));
                    app.set_activity("Awaiting approval", summary);
                }
            }
        }

        terminal.terminal.draw(|frame| render(frame, &app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key(key, &mut app, engine.clone(), tx.clone(), project_root.clone())? {
                        break;
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_key(
    key: KeyEvent,
    app: &mut TuiApp,
    engine: Arc<Mutex<VacEngine>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
    project_root: PathBuf,
) -> anyhow::Result<bool> {
    if let Some(pending) = app.pending_approval.take() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = pending.responder.send(true);
                TuiApp::push_lane(&mut app.control_lane_log, format!("Approved {}", pending.tool_name));
                app.push_transcript(
                    "Approval",
                    format!("Allowed {}", pending.summary),
                    Color::Green,
                );
                app.set_activity("Approved", format!("Allowed {}", pending.summary));
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                let _ = pending.responder.send(false);
                TuiApp::push_lane(&mut app.control_lane_log, format!("Denied {}", pending.tool_name));
                app.push_transcript(
                    "Approval",
                    format!("Denied {}", pending.summary),
                    Color::Red,
                );
                app.set_activity("Denied", format!("Denied {}", pending.summary));
            }
            _ => {
                app.pending_approval = Some(pending);
            }
        }
        return Ok(false);
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.running = false;
        return Ok(true);
    }

    match key.code {
        KeyCode::Char('q') if app.input.is_empty() && app.active_task.is_none() => {
            app.running = false;
            Ok(true)
        }
        KeyCode::Esc => {
            app.input.clear();
            Ok(false)
        }
        KeyCode::Backspace => {
            app.input.pop();
            Ok(false)
        }
        KeyCode::Enter => {
            if app.active_task.is_none() {
                let prompt = app.input.trim().to_string();
                if !prompt.is_empty() {
                    app.input.clear();
                    spawn_task(prompt, engine, tx, project_root);
                }
            }
            Ok(false)
        }
        KeyCode::Tab => {
            app.show_help = !app.show_help;
            Ok(false)
        }
        KeyCode::Char(ch) => {
            app.input.push(ch);
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn spawn_task(
    prompt: String,
    engine: Arc<Mutex<VacEngine>>,
    tx: mpsc::UnboundedSender<TaskEvent>,
    project_root: PathBuf,
) {
    let _ = tx.send(TaskEvent::Started {
        prompt: prompt.clone(),
    });

    tokio::spawn(async move {
        let (updates_tx, mut updates_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
        let updates_forward = tx.clone();
        tokio::spawn(async move {
            while let Some(update) = updates_rx.recv().await {
                let _ = updates_forward.send(TaskEvent::Update(update));
            }
        });

        let mut engine = engine.lock().await;
        let result = engine
            .run_task_with_updates(&prompt, Some(updates_tx))
            .await;
        let status = engine.status().await.unwrap_or_else(|_| fallback_status(project_root.clone()));
        let history = engine.history().await.unwrap_or_default();

        match result {
            Ok(result) => {
                let diff_preview = build_diff_preview(&project_root, &result);
                let _ = tx.send(TaskEvent::Finished {
                    prompt,
                    result,
                    status,
                    history,
                    diff_preview,
                });
            }
            Err(error) => {
                let _ = tx.send(TaskEvent::Failed {
                    prompt,
                    error: error.to_string(),
                    status,
                    history,
                });
            }
        }
    });
}

fn fallback_status(project_root: PathBuf) -> EngineStatus {
    EngineStatus {
        project_root,
        session_id: uuid::Uuid::nil(),
        total_tasks: 0,
        completed_tasks: 0,
        failed_tasks: 0,
        total_tokens_used: 0,
        subsystems_initialized: true,
    }
}

fn render(frame: &mut Frame, app: &TuiApp) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(12),
            Constraint::Length(5),
        ])
        .split(area);

    render_header(frame, vertical[0], app);
    render_body(frame, vertical[1], app);
    render_input(frame, vertical[2], app);

    if app.pending_approval.is_some() {
        render_approval_modal(frame, area, app);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let status_line = if let Some(task) = &app.active_task {
        format!(
            "{} {}",
            spinner_frame(app.spinner_tick),
            truncate(task, 64)
        )
    } else {
        "Idle".to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                "VAC TUI",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(status_line, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(format!(
            "Provider {} | Model {} | Session {}",
            app.active_provider,
            app.active_model,
            app.status.session_id
        )),
        Line::from(format!(
            "Phase {} | Last {}",
            truncate(&app.current_phase, 22),
            truncate(&app.last_activity, 54)
        )),
        Line::from(format!(
            "Tasks {} | Done {} | Failed {} | Tokens {}",
            app.status.total_tasks,
            app.status.completed_tasks,
            app.status.failed_tasks,
            app.status.total_tokens_used
        )),
    ];

    let header = Paragraph::new(lines)
        .block(Block::default().title("Status Bar").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
        .split(area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(12)])
        .split(horizontal[0]);

    render_transcript(frame, left[0], app);
    render_diff_panel(frame, left[1], app);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Length(10), Constraint::Min(9)])
        .split(horizontal[1]);

    render_status_panel(frame, right[0], app);
    render_history_panel(frame, right[1], app);
    lanes::render_lanes(frame, right[2], app);
}

fn render_transcript(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let items: Vec<ListItem> = app
        .transcript
        .iter()
        .rev()
        .take((area.height.saturating_sub(2) as usize).saturating_mul(2))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|entry| {
            ListItem::new(vec![
                Line::from(Span::styled(
                    entry.label.clone(),
                    Style::default()
                        .fg(entry.color)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(entry.body.clone()),
            ])
        })
        .collect();

    let transcript = List::new(items).block(
        Block::default()
            .title("Transcript")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray)),
    );
    frame.render_widget(transcript, area);
}

fn render_diff_panel(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let panel = Paragraph::new(app.diff_preview.clone())
        .block(Block::default().title("Diff / Modified Files").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn render_status_panel(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let mut lines = vec![
        Line::from(format!("Project: {}", app.status.project_root.display())),
        Line::from(format!(
            "Auth: {}",
            if app.auth_ready {
                "ready"
            } else {
                "missing login"
            }
        )),
        Line::from(format!(
            "Subsystems: {}",
            if app.status.subsystems_initialized {
                "ready"
            } else {
                "booting"
            }
        )),
        Line::from(format!(
            "Busy: {}",
            app.active_task
                .as_ref()
                .map(|task| truncate(task, 34))
                .unwrap_or_else(|| "no".to_string())
        )),
        Line::from(format!("Phase: {}", truncate(&app.current_phase, 38))),
        Line::from(format!("Last: {}", truncate(&app.last_activity, 39))),
    ];

    if app.show_help {
        lines.push(Line::from(""));
        lines.push(Line::from("Enter run task"));
        lines.push(Line::from("y / n approve write tool"));
        lines.push(Line::from("Tab toggle help"));
        lines.push(Line::from("q quit"));
        if let Some(hint) = &app.auth_hint {
            lines.push(Line::from(""));
            lines.push(Line::from(truncate(hint, 54)));
        }
    }

    let panel = Paragraph::new(lines)
        .block(Block::default().title("Inspector").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn render_history_panel(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let items: Vec<ListItem> = app
        .history
        .iter()
        .take(area.height.saturating_sub(2) as usize)
        .map(|entry| {
            let status = match &entry.status {
                vac_core::TaskStatus::Completed => "done".to_string(),
                vac_core::TaskStatus::Failed(reason) => truncate(reason, 16),
                other => format!("{:?}", other),
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    truncate(&entry.description, 44),
                    Style::default().fg(Color::White),
                )),
                Line::from(format!("{} | {} tok", status, entry.total_tokens_used)),
            ])
        })
        .collect();

    let history = List::new(items).block(
        Block::default()
            .title("Task History")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(history, area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let block = Block::default()
        .title(if app.active_task.is_some() {
            "Composer (busy)"
        } else {
            "Composer"
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.active_task.is_some() {
            Color::Yellow
        } else {
            Color::Cyan
        }));

    let text = if app.input.is_empty() {
        vec![Line::from(Span::styled(
            "Describe the coding task you want VAC to run...",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        vec![Line::from(app.input.clone())]
    };

    let input = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(Clear, area);
    frame.render_widget(input, area);

    if app.active_task.is_none() && app.pending_approval.is_none() {
        let cursor_x = area.x + app.input.chars().count() as u16 + 1;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_approval_modal(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let Some(pending) = &app.pending_approval else {
        return;
    };

    let modal_area = centered_rect(60, 40, area);
    let content = vec![
        Line::from(Span::styled(
            format!("Approve tool `{}`?", pending.tool_name),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(pending.summary.clone()),
        Line::from(""),
        Line::from(truncate(&pending.args_preview, 280)),
        Line::from(""),
        Line::from("Press y to allow, n or Esc to deny."),
    ];

    frame.render_widget(Clear, modal_area);
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .title("Approval")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: true }),
        modal_area,
    );
}

fn centered_rect(horizontal_percent: u16, vertical_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - vertical_percent) / 2),
            Constraint::Percentage(vertical_percent),
            Constraint::Percentage((100 - vertical_percent) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - horizontal_percent) / 2),
            Constraint::Percentage(horizontal_percent),
            Constraint::Percentage((100 - horizontal_percent) / 2),
        ])
        .flex(Flex::Center)
        .split(vertical[1]);
    horizontal[1]
}

fn truncate(input: &str, max_chars: usize) -> String {
    let mut out = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn spinner_frame(tick: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    FRAMES[tick % FRAMES.len()]
}

fn humanize_status(message: &str) -> String {
    match message {
        "Thinking" => "Thinking about the next step".to_string(),
        other => other.to_string(),
    }
}

fn is_read_tool(tool_name: &str) -> bool {
    matches!(tool_name, "file_read" | "glob" | "grep" | "search")
}

fn transcript_style_for_tool(tool_name: &str) -> (&'static str, Color) {
    match tool_name {
        "file_read" | "glob" | "grep" | "search" => ("Read", Color::Cyan),
        "file_write" | "file_edit" => ("Write", Color::Yellow),
        "bash" | "cargo" | "git" => ("Run", Color::Magenta),
        _ => ("Tool", Color::Magenta),
    }
}

fn activity_phase_for_tool(tool_name: &str) -> String {
    match tool_name {
        "file_write" | "file_edit" => "Editing files".to_string(),
        "bash" | "cargo" | "git" => "Running commands".to_string(),
        _ => "Using tools".to_string(),
    }
}

fn describe_tool_call(name: &str, arguments: &serde_json::Value) -> String {
    match name {
        "file_read" => {
            let path = value_string(arguments, &["path"]).unwrap_or_else(|| "file".to_string());
            let start = arguments.get("start_line").and_then(|v| v.as_u64());
            let end = arguments.get("end_line").and_then(|v| v.as_u64());
            match (start, end) {
                (Some(start), Some(end)) => format!("Read {} lines {}-{}", path, start, end),
                (Some(start), None) => format!("Read {} from line {}", path, start),
                _ => format!("Read {}", path),
            }
        }
        "glob" => {
            let pattern = value_string(arguments, &["pattern"]).unwrap_or_else(|| "*".to_string());
            format!("Find files matching {}", pattern)
        }
        "grep" => {
            let pattern = value_string(arguments, &["pattern"]).unwrap_or_else(|| "pattern".to_string());
            format!("Search with grep for {}", pattern)
        }
        "search" => {
            let query = value_string(arguments, &["query"]).unwrap_or_else(|| "query".to_string());
            format!("Search code for {}", query)
        }
        "bash" => {
            let command = value_string(arguments, &["command"]).unwrap_or_else(|| "command".to_string());
            format!("Run shell command: {}", truncate(&command, 80))
        }
        "cargo" => {
            let command = value_string(arguments, &["command", "args"]).unwrap_or_else(|| "cargo".to_string());
            format!("Run cargo {}", truncate(&command, 74))
        }
        "git" => {
            let command = value_string(arguments, &["command", "args"]).unwrap_or_else(|| "git".to_string());
            format!("Run git {}", truncate(&command, 76))
        }
        "file_write" => {
            let path = value_string(arguments, &["path"]).unwrap_or_else(|| "file".to_string());
            format!("Write {}", path)
        }
        "file_edit" => {
            let path = value_string(arguments, &["file_path"]).unwrap_or_else(|| "file".to_string());
            format!("Edit {}", path)
        }
        "todo_write" => "Update task list".to_string(),
        "task_done" => "Mark task complete".to_string(),
        _ => format!("{} {}", name, truncate(&compact_json(arguments), 96)),
    }
}

fn describe_tool_call_from_preview(name: &str, args_preview: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_preview)
        .map(|value| describe_tool_call(name, &value))
        .unwrap_or_else(|_| format!("Run {} tool", name))
}

fn summarize_tool_result(name: &str, content: &str, success: bool) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(content).ok();
    match name {
        "file_read" => {
            if let Some(value) = parsed {
                let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                let num_lines = value.get("num_lines").and_then(|v| v.as_u64()).unwrap_or(0);
                let start_line = value.get("start_line").and_then(|v| v.as_u64());
                match start_line {
                    Some(start) => format!("Loaded {} lines from {} starting at {}", num_lines, path, start),
                    None => format!("Loaded {} lines from {}", num_lines, path),
                }
            } else {
                format!("File read {}", if success { "completed" } else { "failed" })
            }
        }
        "glob" => {
            if let Some(value) = parsed {
                let total = value.get("total_matches").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("Found {} matching files", total)
            } else {
                "Glob completed".to_string()
            }
        }
        "grep" => {
            if let Some(value) = parsed {
                let total = value.get("total_matches").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("Found {} grep matches", total)
            } else {
                "Grep completed".to_string()
            }
        }
        "search" => {
            if let Some(value) = parsed {
                let total = value.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("Found {} search hits", total)
            } else {
                "Search completed".to_string()
            }
        }
        "bash" | "cargo" | "git" => summarize_command_result(parsed.as_ref(), success),
        "file_write" => {
            if let Some(value) = parsed {
                let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                let created = value.get("created").and_then(|v| v.as_bool()).unwrap_or(false);
                let bytes = value.get("bytes_written").and_then(|v| v.as_u64()).unwrap_or(0);
                if created {
                    format!("Created {} ({} bytes)", path, bytes)
                } else {
                    format!("Wrote {} bytes to {}", bytes, path)
                }
            } else {
                "File write completed".to_string()
            }
        }
        "file_edit" => {
            if let Some(value) = parsed {
                let path = value.get("file_path").and_then(|v| v.as_str()).unwrap_or("file");
                let replaced = value
                    .get("occurrences_replaced")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                format!("Updated {} with {} replacements", path, replaced)
            } else {
                "File edit completed".to_string()
            }
        }
        "task_done" => {
            if success { "Task marked done".to_string() } else { "Task completion failed".to_string() }
        }
        "todo_write" => {
            if success { "Task list updated".to_string() } else { "Task list update failed".to_string() }
        }
        _ => {
            if success {
                truncate(content, 120)
            } else {
                format!("Tool failed: {}", truncate(content, 110))
            }
        }
    }
}

fn summarize_command_result(parsed: Option<&serde_json::Value>, success: bool) -> String {
    if let Some(value) = parsed {
        let exit_code = value.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
        let stdout = value.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
        let stderr = value.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
        let preview = first_non_empty_line(stdout)
            .or_else(|| first_non_empty_line(stderr))
            .unwrap_or_else(|| {
                if success {
                    "No output".to_string()
                } else {
                    "Command failed".to_string()
                }
            });
        format!("Exit {}: {}", exit_code, truncate(&preview, 84))
    } else if success {
        "Command completed".to_string()
    } else {
        "Command failed".to_string()
    }
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn value_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(raw) = value.get(*key) {
            if let Some(text) = raw.as_str() {
                return Some(text.to_string());
            }
            if let Some(array) = raw.as_array() {
                let parts = array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>();
                if !parts.is_empty() {
                    return Some(parts.join(" "));
                }
            }
        }
    }
    None
}

fn build_diff_preview(project_root: &Path, result: &TaskResult) -> String {
    if result.modified_files.is_empty() && result.created_files.is_empty() {
        return "No file changes.".to_string();
    }

    let mut lines = Vec::new();
    if !result.modified_files.is_empty() {
        lines.push(format!("Modified: {}", result.modified_files.join(", ")));
    }
    if !result.created_files.is_empty() {
        lines.push(format!("Created: {}", result.created_files.join(", ")));
    }

    if !result.modified_files.is_empty() {
        let mut args = vec!["-C".to_string(), project_root.display().to_string(), "diff".to_string(), "--unified=2".to_string(), "--".to_string()];
        args.extend(result.modified_files.clone());
        if let Ok(output) = Command::new("git").args(&args).output() {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if !diff.trim().is_empty() {
                lines.push(String::new());
                lines.push(truncate(&diff, 1800));
            }
        }
    }

    lines.join("\n")
}

fn missing_api_key_hint() -> Option<String> {
    match vac_core::auth::auth_status() {
        Ok(status) if status.effective_present => None,
        Ok(_) | Err(_) => {
            Some("VAC is not authenticated. Run `vac auth login`, then restart `vac interactive`.".to_string())
        }
    }
}

fn auth_hint_for_error(error: &str) -> Option<String> {
    if error.contains("API key not found") {
        missing_api_key_hint().or_else(|| {
            Some(
                "The model request failed because no VAC login or provider API key was available."
                    .to_string(),
            )
        })
    } else {
        None
    }
}

struct TuiPolicyEngine {
    sender: Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<TaskEvent>>>>,
    allow: HashMap<String, bool>,
    deny: HashMap<String, bool>,
}

impl TuiPolicyEngine {
    fn new(sender: mpsc::UnboundedSender<TaskEvent>) -> Self {
        let allow = [
            "file_read",
            "glob",
            "grep",
            "search",
            "task_done",
            "todo_write",
        ]
        .into_iter()
        .map(|name| (name.to_string(), true))
        .collect();

        Self {
            sender: Arc::new(std::sync::Mutex::new(Some(sender))),
            allow,
            deny: HashMap::new(),
        }
    }

    fn set_sender(&self, sender: mpsc::UnboundedSender<TaskEvent>) {
        if let Ok(mut guard) = self.sender.lock() {
            *guard = Some(sender);
        }
    }

    fn requires_approval(tool_name: &str) -> bool {
        matches!(tool_name, "file_write" | "file_edit" | "bash" | "git" | "cargo")
    }
}

#[async_trait]
impl PolicyEngine for TuiPolicyEngine {
    async fn decide(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        _context: &ToolContext,
    ) -> PolicyDecision {
        if self.deny.get(tool_name).copied().unwrap_or(false) {
            return PolicyDecision::Deny(format!("Tool {} denied by TUI policy", tool_name));
        }

        if self.allow.get(tool_name).copied().unwrap_or(false) && !Self::requires_approval(tool_name) {
            return PolicyDecision::Allow;
        }

        if !Self::requires_approval(tool_name) {
            return PolicyDecision::Allow;
        }

        let preview = compact_json(args);
        let (responder_tx, responder_rx) = oneshot::channel();
        let sender = self.sender.lock().ok().and_then(|guard| guard.as_ref().cloned());
        if let Some(sender) = sender {
            let _ = sender.send(TaskEvent::ApprovalRequest {
                tool_name: tool_name.to_string(),
                args_preview: preview,
                responder: responder_tx,
            });
            match responder_rx.await {
                Ok(true) => PolicyDecision::Allow,
                Ok(false) => PolicyDecision::Deny(format!("User denied {}", tool_name)),
                Err(_) => PolicyDecision::Deny(format!("Approval channel closed for {}", tool_name)),
            }
        } else {
            PolicyDecision::Deny(format!("No approval channel available for {}", tool_name))
        }
    }
}
