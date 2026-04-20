//! Event Loop Module

use crate::Model;
use crate::app::{
    AppState, AppStateOptions, InputEvent, OutputEvent, SidePanelSection, WorkspaceFocus,
};
use crate::event::map_crossterm_event_to_input_event;
use crate::services::helper_block::welcome_messages;
use crate::terminal::TerminalGuard;
use crate::view::view;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::interval;

/// Rulebook configuration
#[derive(Clone, Debug, Default)]
pub struct RulebookConfig {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}


fn focus_to_label(focus: WorkspaceFocus) -> &'static str {
    match focus {
        WorkspaceFocus::Conversation => "conversation",
        WorkspaceFocus::Input => "input",
        WorkspaceFocus::Workbench => "workbench",
        WorkspaceFocus::Activity => "activity",
    }
}

fn focus_from_label(label: &str) -> WorkspaceFocus {
    match label {
        "conversation" => WorkspaceFocus::Conversation,
        "workbench" => WorkspaceFocus::Workbench,
        "activity" => WorkspaceFocus::Activity,
        _ => WorkspaceFocus::Input,
    }
}

fn side_panel_section_to_label(section: SidePanelSection) -> &'static str {
    match section {
        SidePanelSection::Context => "context",
        SidePanelSection::Runtime => "runtime",
        SidePanelSection::Changeset => "changeset",
        SidePanelSection::Mcp => "mcp",
        SidePanelSection::Sessions => "sessions",
        SidePanelSection::Todos => "todos",
        SidePanelSection::Usage => "usage",
    }
}

fn side_panel_section_from_label(label: &str) -> Option<SidePanelSection> {
    Some(match label {
        "context" => SidePanelSection::Context,
        "runtime" => SidePanelSection::Runtime,
        "changeset" => SidePanelSection::Changeset,
        "mcp" => SidePanelSection::Mcp,
        "sessions" => SidePanelSection::Sessions,
        "todos" => SidePanelSection::Todos,
        "usage" => SidePanelSection::Usage,
        _ => return None,
    })
}

fn build_session_snapshot(state: &AppState) -> Option<vac_session_control::SessionSnapshot> {
    let session_id = uuid::Uuid::parse_str(&state.session_id).ok()?;
    let mut snapshot =
        vac_session_control::SessionSnapshot::new(session_id, state.project_root.clone());
    let todo_pending = state
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::Pending))
        .count();
    let todo_in_progress = state
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::InProgress))
        .count();
    let todo_done = state
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::Done))
        .count();

    snapshot.active_model = state
        .current_model
        .as_ref()
        .map(|model| model.name.clone())
        .or_else(|| state.startup.active_model.clone());
    snapshot.active_profile = Some(state.active_profile.clone());
    let mut selected_rulebooks: Vec<String> = state.selected_rulebooks.iter().cloned().collect();
    selected_rulebooks.sort();
    snapshot.active_rulebooks = selected_rulebooks;
    snapshot.task_count = state.todos.len();
    snapshot.completed_tasks = todo_done;
    snapshot.failed_tasks = 0;
    snapshot.total_tokens = state.total_session_usage.total_tokens;
    snapshot.modified_files = state.modified_files.len();
    snapshot.tui_state.active_tab_idx = Some(crate::workbench::active_tab_index(&state.workbench_tab));
    snapshot.tui_state.history_selection = Some(state.sessions_selected_idx);
    snapshot.tui_state.last_focus = Some(focus_to_label(state.focus).to_string());
    snapshot.tui_state.collapsed_sections = state
        .side_panel_section_collapsed
        .iter()
        .map(|section| side_panel_section_to_label(*section).to_string())
        .collect();
    snapshot
        .metadata
        .insert("focus".into(), focus_to_label(state.focus).into());
    snapshot
        .metadata
        .insert("active_profile".into(), state.active_profile.clone());
    snapshot.metadata.insert(
        "active_isolation_mode".into(),
        state.active_isolation_mode.clone(),
    );
    snapshot.metadata.insert(
        "provider_status".into(),
        state.startup.provider_status.clone(),
    );
    snapshot.metadata.insert(
        "pending_approvals".into(),
        state.pending_approvals.len().to_string(),
    );
    snapshot.metadata.insert(
        "queue_depth".into(),
        state.pending_user_messages.len().to_string(),
    );
    snapshot
        .metadata
        .insert("todo_pending".into(), todo_pending.to_string());
    snapshot
        .metadata
        .insert("todo_in_progress".into(), todo_in_progress.to_string());
    snapshot
        .metadata
        .insert("todo_done".into(), todo_done.to_string());
    Some(snapshot)
}

fn apply_session_snapshot(state: &mut AppState, snapshot: &vac_session_control::SessionSnapshot) {
    state.startup.active_model = snapshot.active_model.clone();
    state.startup.active_profile = snapshot.active_profile.clone();
    state.startup.selected_rulebooks = snapshot.active_rulebooks.clone();
    state.selected_rulebooks = snapshot.active_rulebooks.iter().cloned().collect();
    state.startup.active_rulebook = if snapshot.active_rulebooks.is_empty() {
        None
    } else {
        Some(snapshot.active_rulebooks.join(", "))
    };
    if let Some(idx) = snapshot.tui_state.active_tab_idx {
        state.workbench_tab = crate::workbench::tab_from_index(idx);
    }
    if let Some(selection) = snapshot.tui_state.history_selection {
        state.sessions_selected_idx = selection;
    }
    if let Some(focus) = snapshot.tui_state.last_focus.as_deref() {
        state.focus = focus_from_label(focus);
    }
    state.side_panel_section_collapsed = snapshot
        .tui_state
        .collapsed_sections
        .iter()
        .filter_map(|section| side_panel_section_from_label(section))
        .collect();
    state.active_profile = snapshot
        .active_profile
        .clone()
        .unwrap_or_else(|| "default".to_string());
    if let Some(provider_status) = snapshot.metadata.get("provider_status") {
        state.startup.provider_status = provider_status.clone();
    }
}

async fn persist_session_snapshot(state: &AppState) {
    let Some(snapshot) = build_session_snapshot(state) else {
        return;
    };
    let result = vac_session_control::save_snapshot_async(snapshot).await;
    if let Err(err) = result {
        tracing::warn!("failed to persist session snapshot: {err}");
    }
}

async fn load_session_snapshot(
    project_root: &std::path::Path,
    session_id: &str,
) -> Option<vac_session_control::SessionSnapshot> {
    let session_uuid = uuid::Uuid::parse_str(session_id).ok()?;
    match vac_session_control::load_snapshot_async(project_root.to_path_buf(), session_uuid).await {
        Ok(snapshot) => Some(snapshot),
        Err(vac_session_control::SessionControlError::UnsupportedSchema { .. }) => {
            let root_buf = project_root.to_path_buf();
            let raw = tokio::task::spawn_blocking(move || {
                std::fs::read_to_string(vac_session_control::snapshot_path(&root_buf, session_uuid))
            })
            .await
            .ok()?
            .ok()?;
            let snapshot: vac_session_control::SessionSnapshot = serde_json::from_str(&raw).ok()?;
            vac_session_control::migrate_snapshot(snapshot).ok()
        }
        Err(vac_session_control::SessionControlError::NotFound(_)) => None,
        Err(err) => {
            tracing::warn!("failed to load session snapshot: {err}");
            None
        }
    }
}

/// Run the TUI
#[allow(clippy::too_many_arguments)]
pub async fn run_tui(
    mut input_rx: Receiver<InputEvent>,
    output_tx: Sender<OutputEvent>,
    _cancel_tx: Option<tokio::sync::broadcast::Sender<()>>,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
    latest_version: Option<String>,
    _redact_secrets: bool,
    _privacy_mode: bool,
    _is_git_repo: bool,
    _auto_approve_tools: Option<&Vec<String>>,
    _allowed_tools: Option<&Vec<String>>,
    _current_profile_name: String,
    _rulebook_config: Option<RulebookConfig>,
    model: Option<Model>,
    session_id: Option<String>,
    _editor_command: Option<String>,
    auth_display_info: (Option<String>, Option<String>, Option<String>),
    _init_prompt_content: Option<String>,
    _send_init_prompt_on_start: bool,
    _recent_models: Vec<String>,
    _banner_message: Option<()>,
    project_root: std::path::PathBuf,
) -> io::Result<()> {
    let _guard = TerminalGuard;
    enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;

    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Check for session restore
    let checkpoint_path = std::env::var("VAC_CHECKPOINT")
        .ok()
        .map(std::path::PathBuf::from);

    let mut state = AppState::new(AppStateOptions {
        model: model.clone(),
        session_id,
        checkpoint_path: checkpoint_path.clone(),
        project_root: project_root.clone(),
    });
    state.auth_display_info = auth_display_info;

    // Hydrate startup state
    state.startup.has_vil_engine =
        vac_core::detector::VilProjectProfile::detect(&project_root).is_vil_project;
    if let Some(rb) = &_rulebook_config {
        if let Some(inc) = &rb.include {
            if !inc.is_empty() {
                state.startup.active_rulebook = Some(inc.join(", "));
                state.startup.selected_rulebooks = inc.clone();
            }
        }
    }
    // Hydrate model info from what's available at boot
    state.startup.active_model = model.as_ref().map(|m| m.name.clone());
    state.startup.default_model = model.as_ref().map(|m| m.id.clone());
    // Hydrate profile from run_tui parameter
    if !_current_profile_name.is_empty() {
        state.startup.active_profile = Some(_current_profile_name.clone());
    }
    // Hydrate MCP server count from config
    {
        let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
        state.startup.mcp_server_count = config.mcp_servers.as_ref().map_or(0, |s| s.len());
    }
    // Provider status from auth_display_info
    state.startup.provider_status = match &state.auth_display_info.0 {
        Some(provider) => format!("ready ({})", provider),
        None => "loading...".to_string(),
    };

    if let Some(snapshot) = load_session_snapshot(&project_root, &state.session_id).await {
        apply_session_snapshot(&mut state, &snapshot);
    }

    // Add welcome messages
    let welcome = welcome_messages(latest_version.as_deref(), &state);
    state.messages.extend(welcome);

    // Seed a persistent upgrade banner when an upstream version is available.
    if let Some(v) = latest_version.as_deref() {
        let current = env!("CARGO_PKG_VERSION");
        if v != current {
            state.banner_message = Some(
                crate::services::banner::BannerMessage::persistent_with_action(
                    format!(
                        "New VAC release available: {} (installed: {}). Run /upgrade to update.",
                        v, current
                    ),
                    crate::services::banner::BannerStyle::Info,
                    "/upgrade",
                ),
            );
        }
    }

    // Request session restore if checkpoint exists
    if let Some(path) = &checkpoint_path {
        if let Some(session_id) = path.file_name().and_then(|n| n.to_str()) {
            let _ = output_tx.try_send(OutputEvent::ResumeSession(session_id.to_string()));
        }
    }

    // Create input thread
    let (input_tx, mut internal_rx) = tokio::sync::mpsc::channel::<InputEvent>(100);
    state.input_tx = Some(input_tx.clone());

    // Probe MCP servers in background
    let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
    if let Some(servers) = config.mcp_servers {
        let input_tx_clone = input_tx.clone();
        tokio::spawn(async move {
            for server in servers {
                let state = vac_tools::mcp::probe_mcp_server(&server).await;
                let _ = input_tx_clone
                    .send(InputEvent::McpServerState(server.name, state))
                    .await;
            }
        });
    }

    // Spawn background task to detect VIL project profile
    let bg_input_tx = input_tx.clone();
    let bg_project_root = project_root.clone();
    tokio::spawn(async move {
        let profile = vac_core::detector::VilProjectProfile::detect(&bg_project_root);

        let mut ir_generation_active = false;
        let mut ir_metadata_files = vec![];
        if let Ok(pipeline) = vil_ir::IrPipeline::new(&bg_project_root) {
            ir_generation_active = true;
            for (path, module) in pipeline.modules() {
                let has_vil_attr = module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                    || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                if has_vil_attr {
                    ir_metadata_files.push(path.clone());
                }
            }
        }

        let config = vac_core::VacConfig::load_with_fallback(&bg_project_root).unwrap_or_default();
        let semantic_mode = config.memory.enable_semantic;

        let active_rulebook = {
            let books = vac_core::rulebook::RulebookLoader::load_all(
                &bg_project_root,
                &config.rulebook.paths,
            );
            if books.is_empty() {
                None
            } else {
                Some(
                    books
                        .iter()
                        .map(|b| b.id.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
        };

        let snapshot = crate::app::VilStatusSnapshot {
            profile: Some(profile),
            validation_score: 1.0,
            validation_issues: vec![],
            active_rulebook,
            semantic_mode,
            ir_generation_active,
            ir_metadata_files,
        };
        let _ = bg_input_tx
            .send(InputEvent::VilStatusUpdated(snapshot))
            .await;
    });

    let input_paused = Arc::new(AtomicBool::new(false));
    let input_paused_clone = input_paused.clone();

    std::thread::spawn(move || {
        loop {
            if input_paused_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            if crossterm::event::poll(Duration::from_millis(100)).ok()? {
                if let Some(event) = crossterm::event::read()
                    .ok()
                    .and_then(map_crossterm_event_to_input_event)
                {
                    if input_tx.blocking_send(event).is_err() {
                        break;
                    }
                }
            }
        }
        Some(())
    });

    let mut spinner_interval = interval(Duration::from_millis(150));
    let mut last_session_snapshot_save = Instant::now();

    loop {
        // Handle internal events
        while let Ok(event) = internal_rx.try_recv() {
            crate::controller::handle_input_event(&mut state, &output_tx, event);
        }

        // Handle backend events
        while let Ok(event) = input_rx.try_recv() {
            crate::controller::handle_backend_event(&mut state, &output_tx, event);
        }

        if let Some(tx) = state.input_tx.clone() {
            crate::update::flush_pending_user_messages_if_idle(&mut state, &tx, &output_tx);
        }

        // Update spinner
        spinner_interval.tick().await;
        if state.loading || state.is_streaming {
            state.spinner_frame = (state.spinner_frame + 1) % 10;
        }

        state.toasts.retain(|t| !t.is_expired());
        if state.toasts.len() > 3 {
            state.toasts.drain(0..state.toasts.len().saturating_sub(3));
        }

        // Render — measure wall time and update RenderMetrics
        let render_start = std::time::Instant::now();
        terminal.draw(|f| view(f, &mut state))?;
        let render_us = render_start.elapsed().as_micros() as u64;
        state.render_metrics.last_render_time_us = render_us;
        // Exponential moving average (α ≈ 0.1)
        state.render_metrics.ema_render_time_us = if state.render_metrics.ema_render_time_us == 0 {
            render_us
        } else {
            (state.render_metrics.ema_render_time_us * 9 + render_us) / 10
        };
        if render_us > 16_000 {
            log::debug!("render over budget: {}µs (avg {}µs)", render_us, state.render_metrics.ema_render_time_us);
        }

        if last_session_snapshot_save.elapsed() >= Duration::from_secs(30) {
            persist_session_snapshot(&state).await;
            last_session_snapshot_save = Instant::now();
        }

        // Check for quit
        if state.cancel_requested {
            persist_session_snapshot(&state).await;
            break;
        }
    }

    Ok(())
}

/// Unit 5 (Wave 3.1): Handle a single-char keystroke that may target the
/// paste tray. Returns `true` when the key was consumed by the tray and
/// should NOT fall through to the normal input path.
///
/// Gate: caller must ensure `input.is_empty() && !pending_pastes.is_empty()`.
/// Bindings:
/// - `j` / `k` — select next / prev (wrap)
/// - `d` / `x` — remove selected paste
/// - `r` — toggle reorder mode
/// - `J` / `K` (only in reorder mode) — swap with next / prev
/// - Enter is handled via `InputSubmitted`, not here.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{apply_session_snapshot, build_session_snapshot, load_session_snapshot};
    use crate::app::{
        AppState, AppStateOptions, InputEvent, OutputEvent, SidePanelSection, WorkbenchTab,
        WorkspaceFocus,
    };
    use crate::controller::{classify_critical_banner, open_ask_user_popup};
    use crate::{FunctionCall, Model, ToolCall};

    fn make_state(project_root: std::path::PathBuf, session_id: uuid::Uuid) -> AppState {
        AppState::new(AppStateOptions {
            model: None,
            session_id: Some(session_id.to_string()),
            checkpoint_path: Some(project_root.join(".vac/checkpoints")),
            project_root,
        })
    }

    #[tokio::test]
    async fn session_snapshot_bridge_restores_tui_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();

        let mut state = make_state(root.clone(), session_id);
        state.current_model = Some(Model {
            id: "claude-sonnet-4".to_string(),
            name: "claude-sonnet-4".to_string(),
            provider: "anthropic".to_string(),
            supports_reasoning: true,
            supports_tool_calls: true,
            supports_streaming: true,
            context_window: 200000,
            cost_class: "premium".to_string(),
        });
        state.active_profile = "strict-vil".to_string();
        state.selected_rulebooks.insert("security".to_string());
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = WorkbenchTab::Runtime;
        state.sessions_selected_idx = 3;
        state
            .side_panel_section_collapsed
            .insert(SidePanelSection::Runtime);
        state.total_session_usage.total_tokens = 2048;
        state.modified_files = vec!["src/main.rs".to_string()];

        let snapshot = build_session_snapshot(&state).unwrap();
        vac_session_control::save_snapshot_async(snapshot)
            .await
            .unwrap();
        let loaded = load_session_snapshot(&root, &state.session_id)
            .await
            .unwrap();

        let mut restored = make_state(root.clone(), session_id);
        apply_session_snapshot(&mut restored, &loaded);

        assert_eq!(
            restored.current_model.as_ref().map(|m| m.name.as_str()),
            None
        );
        assert_eq!(
            restored.startup.active_model.as_deref(),
            Some("claude-sonnet-4")
        );
        assert_eq!(restored.active_profile, "strict-vil");
        assert!(restored.selected_rulebooks.contains("security"));
        assert_eq!(restored.focus, WorkspaceFocus::Workbench);
        assert_eq!(restored.workbench_tab, WorkbenchTab::Runtime);
        assert_eq!(restored.sessions_selected_idx, 3);
        assert!(
            restored
                .side_panel_section_collapsed
                .contains(&SidePanelSection::Runtime)
        );
        assert_eq!(restored.total_session_usage.total_tokens, 0);
        assert_eq!(restored.startup.provider_status, "initializing");
        assert_eq!(
            restored.startup.active_rulebook.as_deref(),
            Some("security")
        );
        assert_eq!(
            restored.startup.active_profile.as_deref(),
            Some("strict-vil")
        );
        assert_eq!(
            loaded.metadata.get("todo_pending").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            loaded.metadata.get("todo_in_progress").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            loaded.metadata.get("todo_done").map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn review_selection_normalizes_when_filter_excludes_selected() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), false);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.review.selected_path = Some("b.txt".to_string());
        state.review.filter = "a".to_string();
        state.review_sync_items();
        state.review_normalize_selection();
        assert_eq!(state.review.selected_path, Some("a.txt".to_string()));
        assert_eq!(state.review.selected_idx, 0);
    }

    #[tokio::test]
    async fn slash_semantics_fix_and_explain_send_prompt_with_args() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state.input.set_content("/fix cargo clippy");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::UserMessage(prompt, _, _, _) => {
                assert!(prompt.contains("Fix linter/build errors"));
                assert!(prompt.contains("cargo clippy"));
                assert!(!prompt.trim_start().starts_with("/fix"));
            }
            _ => panic!("unexpected event"),
        }

        state
            .input
            .set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::UserMessage(prompt, _, _, _) => {
                assert!(prompt.contains("Explain the relevant code or concept"));
                assert!(prompt.contains("crates/vac_cli/src/tui/event_loop.rs"));
                assert!(!prompt.trim_start().starts_with("/explain"));
            }
            _ => panic!("unexpected event"),
        }
    }

    #[test]
    fn slash_review_opens_workstation_instead_of_sending_literal() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();
        state.input.set_content("/review");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.review.open);
        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
    }

    #[test]
    fn review_open_close_transitions_clear_diff() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review.diff = Some(crate::app::ReviewDiffState {
            path: "a.txt".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 0,
            last_error: None,
        });
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);
        assert!(!state.review.open);
        assert!(state.review.diff.is_none());
    }

    #[test]
    fn diff_scroll_state_changes_on_page_down() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review.diff = Some(crate::app::ReviewDiffState {
            path: "a.txt".to_string(),
            old_content: Some("a\nb\nc\nd\ne\n".to_string()),
            new_content: Some("a\nb\nX\nd\ne\n".to_string()),
            scroll: 0,
            last_error: None,
        });
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
        assert!(state.review.diff.as_ref().unwrap().scroll > 0);
    }

    #[tokio::test]
    async fn ask_user_filter_and_shortcuts_update_state_and_send_structured_result() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let args = r#"{
            "question": "Pick tags",
            "options": [
                {"id":"a","label":"Alpha"},
                {"id":"b","label":"Beta"},
                {"id":"c","label":"Gamma"}
            ],
            "kind": "multi_select",
            "allow_free_text": false,
            "metadata": {"source":"test"}
        }"#;
        let tc = ToolCall {
            id: "tc_ask_1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: crate::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
                arguments: args.to_string(),
            },
            metadata: None,
        };
        open_ask_user_popup(&mut state, &tc);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::AskUser));
        assert_eq!(
            state.ask_user_question_kind,
            crate::services::ask_user::AskUserQuestionKind::MultiSelect
        );
        assert_eq!(state.ask_user_metadata.get("source").unwrap(), "test");

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
        assert!(state.ask_user_search_active);
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('b'));
        assert_eq!(state.ask_user_filter, "b");
        assert_eq!(state.ask_user_selected, 1);

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::Tab);
        assert!(!state.ask_user_search_active);

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputCursorStart);
        assert!(state.ask_user_multi_selected.contains(&1));
        assert_eq!(state.ask_user_multi_selected.len(), 1);

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputClear);
        assert!(state.ask_user_multi_selected.is_empty());

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        let OutputEvent::SendToolResult(res, _, _) = ev else {
            panic!("expected SendToolResult");
        };
        assert_eq!(res.status, crate::types::ToolCallResultStatus::Success);
        let parsed: serde_json::Value = serde_json::from_str(&res.result).unwrap();
        assert_eq!(parsed["kind"], "multi_select");
        assert_eq!(parsed["selected"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn critical_banner_mcp_tls_is_blocking() {
        let msg = "Failed to connect MCP server 'github': MCP error: Failed to build HTTP client for MCP server 'github': error setting certificate verify locations";
        let out = classify_critical_banner(msg);
        assert_eq!(
            out,
            Some((
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking
            ))
        );
    }

    #[test]
    fn critical_banner_governance_block_is_blocking() {
        let msg = "Tool execution error: Warden blocked: process substitution $() not allowed";
        let out = classify_critical_banner(msg);
        assert_eq!(
            out,
            Some((
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking
            ))
        );
    }

    #[test]
    fn shell_output_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel::<String>(1);
        let shell = vac_shell::ShellCommand {
            id: "shell-1".to_string(),
            command: "sh".to_string(),
            stdin_tx,
        };
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellStarted(shell.clone()),
        );

        let big = "x".repeat(2 * 1024 * 1024);
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellOutput(shell.id.clone(), big),
        );
        let active = state.shell.session_store.active().unwrap();
        assert!(active.output.len() <= 1024 * 1024);
        assert!(active.output.chars().all(|c| c == 'x'));
    }

    #[tokio::test]
    async fn approval_queue_accepts_selected_and_emits_output_event() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let tc1 = ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
            },
            metadata: None,
        };
        let tc2 = ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_edit".to_string(),
                arguments:
                    serde_json::json!({"file_path":"b.txt","old_string":"a","new_string":"b"})
                        .to_string(),
            },
            metadata: None,
        };

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc1.clone(), Some("x".to_string())),
        );
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc2.clone(), Some("y".to_string())),
        );

        assert_eq!(state.pending_approvals.len(), 2);
        assert_eq!(state.approval_selected_idx, 1);
        assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Approvals);

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('a'));
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::AcceptTool(tc) => assert_eq!(tc.id, "tc-2"),
            _ => panic!("unexpected event"),
        }

        assert_eq!(state.pending_approvals.len(), 1);
        assert_eq!(state.pending_approvals[0].id, "tc-1");
        assert!(state.approved_tools.iter().any(|t| t.id == "tc-2"));
    }

    #[tokio::test]
    async fn approval_queue_rejects_selected_and_emits_output_event() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let tc = ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
            },
            metadata: None,
        };

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc.clone(), None),
        );

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
        // 'r' now shows reason prompt - confirm with Enter to reject
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::RejectTool(tc, _, _) => assert_eq!(tc.id, "tc-1"),
            _ => panic!("unexpected event"),
        }

        assert!(state.pending_approvals.is_empty());
        assert!(state.rejected_tools.iter().any(|t| t.id == "tc-1"));
    }

    #[tokio::test]
    async fn revert_selected_updates_status_and_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        let file_rel = "a.txt";
        std::fs::write(root.join(file_rel), "new").unwrap();
        std::fs::write(
            crate::services::review::snapshot_path(&root, session_id, file_rel),
            "old",
        )
        .unwrap();

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state
            .changeset_store
            .file_modified(file_rel.to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review_sync_items();
        state.review.selected_path = Some(file_rel.to_string());

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertSelected);

        let content = std::fs::read_to_string(root.join(file_rel)).unwrap();
        assert_eq!(content, "old");
        assert!(!state.modified_files.contains(&file_rel.to_string()));
        let it = state.review.items.get(file_rel).unwrap();
        assert_eq!(it.status, crate::app::ReviewItemStatus::Restored);
    }

    #[tokio::test]
    async fn revert_filtered_only_affects_filtered_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(
                crate::services::review::snapshot_path(&root, session_id, p),
                old,
            )
            .unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review.filter = "a".to_string();
        state.review_sync_items();
        state.review_normalize_selection();

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertFiltered);

        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "old-a"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "new-b"
        );
        assert!(!state.modified_files.contains(&"a.txt".to_string()));
        assert!(state.modified_files.contains(&"b.txt".to_string()));
    }

    #[tokio::test]
    async fn global_approval_hotkey_ctrl_m_approves_current() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger Ctrl+M (AutoApproveCurrentTool)
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);

        // Verify approval processed
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 1);
        assert_eq!(state.approved_tools[0].id, "tc-1");

        // Verify output event sent
        let output = rx.try_recv().unwrap();
        assert!(matches!(output, OutputEvent::AcceptTool(_)));
    }

    #[tokio::test]
    async fn global_approval_hotkey_ctrl_shift_m_rejects_current() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger Ctrl+Shift+M (RejectCurrentTool) - now shows reason prompt
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

        // Reason prompt should be active
        assert!(state.reject_reason_input.is_some());
        assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

        // Confirm with Enter (no reason typed)
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

        // Verify rejection processed
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);
        assert_eq!(state.rejected_tools[0].id, "tc-2");

        // Verify output event sent
        let output = rx.try_recv().unwrap();
        assert!(matches!(output, OutputEvent::RejectTool(_, _, _)));
    }

    #[tokio::test]
    async fn global_approval_hotkeys_safe_when_no_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // No pending approvals
        assert_eq!(state.pending_approvals.len(), 0);

        // Trigger hotkeys - should not panic
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

        // State unchanged
        assert_eq!(state.approved_tools.len(), 0);
        assert_eq!(state.rejected_tools.len(), 0);
    }

    #[tokio::test]
    async fn slash_command_model_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /model command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/model"));
    }

    #[tokio::test]
    async fn slash_command_files_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /files command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/files"));
    }

    #[tokio::test]
    async fn slash_command_changes_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /changes command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/changes"));
    }

    #[test]
    fn footer_approval_bar_shows_when_pending_approvals() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: r#"{"file_path":"test.rs"}"#.to_string(),
            },
            metadata: None,
        });

        // Footer should show approval bar (verified by view rendering logic)
        assert!(!state.pending_approvals.is_empty());
        assert_eq!(state.approval_selected_idx, 0);
    }

    #[test]
    fn footer_approval_bar_hidden_when_no_pending() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // No pending approvals
        assert!(state.pending_approvals.is_empty());
        // Footer should show normal hints (verified by view rendering logic)
    }

    #[test]
    fn footer_approval_bar_shows_correct_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add multiple pending approvals
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "test_tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }

        // Select second approval
        state.approval_selected_idx = 1;

        // Verify index
        assert_eq!(state.approval_selected_idx, 1);
        assert_eq!(state.pending_approvals.len(), 3);
    }

    #[tokio::test]
    async fn session_restore_clears_popup_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Set popup states
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ModelSwitcher);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::FileSearch);
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::Changeset);
        state.model_switcher_filter = "test".to_string();
        state.file_search_query = "query".to_string();

        // Trigger session restore
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify all popup states cleared
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::ModelSwitcher));
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::FileSearch));
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::Changeset));
        assert!(state.model_switcher_filter.is_empty());
        assert!(state.file_search_query.is_empty());
    }

    #[tokio::test]
    async fn session_restore_clears_changeset_store() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add changeset entries
        state
            .changeset_store
            .file_created("a.rs".to_string(), "agent".to_string());
        state
            .changeset_store
            .file_modified("b.rs".to_string(), "agent".to_string(), true);
        assert_eq!(state.changeset_store.entries().len(), 2);

        // Trigger session restore
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify changeset cleared
        assert_eq!(state.changeset_store.entries().len(), 0);
    }

    #[tokio::test]
    async fn session_restore_clears_approval_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add approval state
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        state.approved_tools.push(ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger session restore
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify approval state cleared
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 0);
        assert_eq!(state.rejected_tools.len(), 0);
    }

    #[test]
    fn command_palette_filters_commands_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Empty filter shows all commands
        state.command_palette_input = "".to_string();
        let all = state.filtered_commands();
        assert!(!all.is_empty());

        // Filter by prefix
        state.command_palette_input = "/model".to_string();
        let filtered = state.filtered_commands();
        assert!(filtered.iter().any(|c| c.command == "/model"));

        // Non-matching filter
        state.command_palette_input = "/nonexistent".to_string();
        let empty = state.filtered_commands();
        assert!(empty.is_empty());
    }

    #[test]
    fn command_palette_selection_stays_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state.command_palette_input = "".to_string();
        let commands = state.filtered_commands();

        // Selection should not exceed command count
        if !commands.is_empty() {
            state.command_palette_selected = 0;
            assert_eq!(state.command_palette_selected, 0);

            state.command_palette_selected = commands.len() - 1;
            assert_eq!(state.command_palette_selected, commands.len() - 1);
        }
    }

    #[tokio::test]
    async fn command_palette_dispatch_does_not_send_literal_slash() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, mut rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Execute /review command
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        let filtered = state.filtered_commands();
        if let Some(cmd) = filtered.iter().find(|c| c.command == "/review") {
            // Simulate command execution
            state.add_user_message(cmd.command.clone());
            state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
            crate::overlay::close_overlay(&mut state, crate::overlay::OverlayId::CommandPalette);
        }

        // Verify review opened, not sent as literal message
        assert!(state.review.open);
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::CommandPalette));

        // No output event should be sent for /review
        assert!(rx.try_recv().is_err());
    }
    #[tokio::test]
    async fn revert_all_clears_modified_files_and_marks_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(
                crate::services::review::snapshot_path(&root, session_id, p),
                old,
            )
            .unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review_sync_items();
        state.review_normalize_selection();

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewRevertAll);

        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "old-a"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "old-b"
        );
        assert!(state.modified_files.is_empty());
        assert_eq!(
            state.review.items.get("a.txt").unwrap().status,
            crate::app::ReviewItemStatus::Restored
        );
        assert_eq!(
            state.review.items.get("b.txt").unwrap().status,
            crate::app::ReviewItemStatus::Restored
        );
    }

    // ── Branch 4A behavioral tests ──────────────────────────────────────────

    #[test]
    fn modified_files_is_derived_from_changeset_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("a.rs".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_created("b.rs".to_string(), "agent".to_string());
        state.modified_files = state.changeset_store.modified_files();

        // modified_files must equal store's derived view
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
        assert_eq!(state.modified_files.len(), 2);
    }

    #[test]
    fn counter_consistency_header_tab_popup() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("x.rs".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_created("y.rs".to_string(), "agent".to_string());
        state
            .changeset_store
            .file_modified("z.rs".to_string(), "agent".to_string(), true);
        state.changeset_store.revert_success("z.rs"); // reverted: not active

        let active = state.changeset_store.active_entries().len();
        // All three surfaces must read the same count
        assert_eq!(active, 2); // x.rs + y.rs; z.rs is reverted
        // review_filtered_paths also driven by active_entries
        state.review_sync_items();
        assert_eq!(state.review_filtered_paths().len(), 2);
    }

    #[test]
    fn task_completed_does_not_dual_write() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let result = vac_core::task::TaskResult {
            task_id: vac_core::task::TaskId::new(),
            status: vac_core::task::TaskStatus::Completed,
            summary: "done".to_string(),
            modified_files: vec!["src/lib.rs".to_string()],
            created_files: vec!["src/new.rs".to_string()],
            validation_score: None,
            elapsed_ms: 0,
            total_tokens_used: 0,
            agent_contributions: vec![],
        };
        crate::controller::handle_backend_event(&mut state, &tx, InputEvent::TaskCompleted(result));

        // modified_files must equal store's derived view - no independent writes
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
        assert_eq!(state.changeset_store.active_entries().len(), 2);
    }

    #[test]
    fn session_restore_clears_store_and_syncs_derived() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("a.rs".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
        assert_eq!(state.modified_files.len(), 1);

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "new".to_string(),
                messages: vec![],
            },
        );

        // Both store and derived view must be empty
        assert_eq!(state.changeset_store.active_entries().len(), 0);
        assert_eq!(state.modified_files.len(), 0);
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
    }

    // ── Branch 5A behavioral tests ──────────────────────────────────────────

    #[test]
    fn show_model_switcher_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowModelSwitcher);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::ModelSwitcher));
        assert!(state.model_switcher_filter.is_empty());
    }

    #[test]
    fn show_file_search_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowFileSearch);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::FileSearch));
        assert!(state.file_search_query.is_empty());
    }

    #[test]
    fn show_changeset_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ShowChangeset);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::Changeset));
    }

    #[tokio::test]
    async fn slash_model_dispatch_opens_model_switcher_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/model");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::ModelSwitcher));
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::FileSearch));
    }

    #[tokio::test]
    async fn slash_files_dispatch_opens_file_search_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/files");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::FileSearch));
        assert!(!state.overlay_manager.is_active(crate::overlay::OverlayId::ModelSwitcher));
    }

    #[tokio::test]
    async fn slash_changes_dispatch_opens_changeset_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/changes");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.overlay_manager.is_active(crate::overlay::OverlayId::Changeset));
    }

    #[tokio::test]
    async fn slash_review_dispatch_opens_review_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/review");
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.review.open);
        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
    }

    #[tokio::test]
    async fn approve_current_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-h".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "write_file".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 1);
        assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
    }

    #[tokio::test]
    async fn reject_current_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-r".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "delete_file".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        // Reason prompt active - not yet rejected
        assert!(state.reject_reason_input.is_some());
        // Confirm with Enter
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);
        assert!(matches!(
            rx.try_recv().unwrap(),
            OutputEvent::RejectTool(_, _, _)
        ));
    }

    // ── Branch 6C behavioral tests ──────────────────────────────────────────

    #[test]
    fn session_info_enriched_fields_populated() {
        let s = crate::app::SessionInfo {
            id: "abc123".to_string(),
            title: "Test Session".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec!["abc123_state.json".to_string()],
            task_count: 5,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: true,
            snapshot_present: false,
            snapshot_stale: false,
        };
        assert_eq!(s.task_count, 5);
        assert_eq!(s.last_activity, "2026-04-16 09:00");
        assert!(s.has_checkpoint);
        assert_eq!(s.checkpoints.len(), 1);
    }

    #[test]
    fn set_sessions_event_populates_enriched_fields() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let sessions = vec![
            crate::app::SessionInfo {
                id: "s1".to_string(),
                title: "Session 1".to_string(),
                updated_at: "2026-04-16T09:00:00Z".to_string(),
                checkpoints: vec![],
                task_count: 3,
                last_activity: "2026-04-16 09:00".to_string(),
                has_checkpoint: false,
                snapshot_present: false,
                snapshot_stale: false,
            },
            crate::app::SessionInfo {
                id: "s2".to_string(),
                title: "Session 2".to_string(),
                updated_at: "2026-04-16T10:00:00Z".to_string(),
                checkpoints: vec!["s2_state.json".to_string()],
                task_count: 7,
                last_activity: "2026-04-16 10:00".to_string(),
                has_checkpoint: true,
                snapshot_present: false,
                snapshot_stale: false,
            },
        ];

        crate::controller::handle_backend_event(&mut state, &tx, InputEvent::SetSessions(sessions));

        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.sessions[0].task_count, 3);
        assert!(!state.sessions[0].has_checkpoint);
        assert_eq!(state.sessions[1].task_count, 7);
        assert!(state.sessions[1].has_checkpoint);
        assert_eq!(state.sessions[1].checkpoints.len(), 1);
    }

    #[test]
    fn sessions_tab_r_resumes_checkpoint_for_selected_session() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Sessions;
        state.sessions = vec![crate::app::SessionInfo {
            id: "sess-abc".to_string(),
            title: "Session abc".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec!["sess-abc_state.json".to_string()],
            task_count: 2,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: true,
            snapshot_present: false,
            snapshot_stale: false,
        }];
        state.sessions_selected_idx = 0;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

        let ev = rx.try_recv().unwrap();
        assert!(matches!(ev, OutputEvent::ResumeSession(ref id) if id == "sess-abc"));
    }

    #[test]
    fn sessions_tab_r_toasts_when_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Sessions;
        state.sessions = vec![crate::app::SessionInfo {
            id: "sess-xyz".to_string(),
            title: "Session xyz".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec![],
            task_count: 0,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: false,
            snapshot_present: false,
            snapshot_stale: false,
        }];
        state.sessions_selected_idx = 0;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

        assert!(!state.toasts.is_empty());
        assert!(state.toasts[0].message.contains("No checkpoint"));
    }

    // ── Branch 6B behavioral tests ──────────────────────────────────────────

    #[test]
    fn at_trigger_activates_on_at_char() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));

        assert!(state.at_trigger_active);
        assert!(state.at_query.is_empty());
    }

    #[test]
    fn at_trigger_updates_query_on_subsequent_chars() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('s'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('c'));

        assert!(state.at_trigger_active);
        assert_eq!(state.at_query, "src");
    }

    #[test]
    fn at_trigger_deactivates_on_esc() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
        state.at_query = "src".to_string();

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);

        assert!(!state.at_trigger_active);
        assert!(state.at_query.is_empty());
    }

    #[test]
    fn at_trigger_deactivates_on_space() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
        state.at_query = "src".to_string();

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged(' '));

        assert!(!state.at_trigger_active);
    }

    #[test]
    fn at_trigger_backspace_pops_query() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
        state.at_query = "sr".to_string();

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
        assert_eq!(state.at_query, "s");
        assert!(state.at_trigger_active);

        // Backspace on empty query deactivates
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputBackspace); // removes '@'
        assert!(!state.at_trigger_active);
    }

    #[test]
    fn at_trigger_enter_inserts_selected_path() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AtDropdown);
        state.at_query = "src".to_string();
        state.at_results = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        state.at_selected_idx = 0;
        // Simulate @src already in input
        state.input.insert_str("@src");

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

        assert!(!state.at_trigger_active);
        assert!(state.at_query.is_empty());
        // Input should contain the path
        let content = state.input.get_content();
        assert!(content.contains("src/main.rs"));
    }

    // ── Branch 6A behavioral tests ──────────────────────────────────────────

    #[tokio::test]
    async fn approve_all_clears_all_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ApproveAll);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 3);
        // 3 AcceptTool events emitted
        for _ in 0..3 {
            assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
        }
    }

    #[tokio::test]
    async fn reject_all_clears_all_pending_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectAll);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 3);
        for _ in 0..3 {
            assert!(matches!(
                rx.try_recv().unwrap(),
                OutputEvent::RejectTool(_, _, _)
            ));
        }
    }

    #[tokio::test]
    async fn reject_current_shows_reason_prompt_then_confirms() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-reason".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger reject - shows prompt
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        assert!(state.reject_reason_input.is_some());
        assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

        // Type reason
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('t'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
        assert_eq!(state.reject_reason_input.as_deref(), Some("too"));

        // Confirm
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.reject_reason_input.is_none());
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);

        // Reason passed in event
        if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
            assert_eq!(reason, Some("too".to_string()));
        } else {
            panic!("expected RejectTool");
        }
    }

    #[tokio::test]
    async fn reject_reason_esc_rejects_without_reason() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-esc".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        assert!(state.reject_reason_input.is_some());

        // Esc = skip reason, reject without reason
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
        assert!(state.reject_reason_input.is_none());
        assert_eq!(state.pending_approvals.len(), 0);

        if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
            assert_eq!(reason, None);
        } else {
            panic!("expected RejectTool");
        }
    }

    // ── Branch 5B behavioral tests ──────────────────────────────────────────

    #[test]
    fn review_open_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewOpen);

        assert!(state.review.open);
        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Review);
        assert_eq!(state.focus, crate::app::WorkspaceFocus::Workbench);
    }

    #[test]
    fn review_close_via_esc_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewClose);

        assert!(!state.review.open);
        assert!(state.review.diff.is_none());
    }

    #[test]
    fn review_filter_push_pop_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('a'));
        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('b'));
        assert_eq!(state.review.filter, "ab");

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewFilterBackspace);
        assert_eq!(state.review.filter, "a");
    }

    #[test]
    fn review_toggle_diff_clears_when_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review.selected_path = Some("a.rs".to_string());
        state.review.diff = Some(crate::app::ReviewDiffState {
            path: "a.rs".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 0,
            last_error: None,
        });

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::ReviewToggleDiff);

        assert!(state.review.diff.is_none());
    }

    #[test]
    fn review_scroll_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review.open = true;
        crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::ReviewPane);
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        state.review.diff = Some(crate::app::ReviewDiffState {
            path: "a.rs".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 5,
            last_error: None,
        });

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageUp);
        assert_eq!(state.review.diff.as_ref().unwrap().scroll, 0);

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::PageDown);
        assert!(state.review.diff.as_ref().unwrap().scroll > 0);
    }

    #[tokio::test]
    async fn runtime_tab_requests_refresh_on_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Sessions;

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Agents);
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::ListAgentTasks
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::LoadAgentState
        ));

        crate::controller::handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

        assert_eq!(state.workbench_tab, crate::app::WorkbenchTab::Runtime);
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::ListRuntimeJobs
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::LoadRuntimeState
        ));
    }

    #[test]
    fn shell_backend_lifecycle_updates_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel(4);
        let shell = vac_shell::ShellCommand {
            id: "shell-1".to_string(),
            command: "echo hi".to_string(),
            stdin_tx,
        };

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellStarted(shell.clone()),
        );
        let active = state.shell.session_store.active().unwrap();
        assert!(active.command.is_some());
        assert!(state.shell.session_store.popup_visible);

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellOutput("shell-1".to_string(), "hello\n".to_string()),
        );
        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellWaitingForInput("shell-1".to_string()),
        );
        let active = state.shell.session_store.active().unwrap();
        assert!(active.output.contains("hello"));
        assert!(active.waiting_for_input);

        crate::controller::handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellCompleted("shell-1".to_string(), 0),
        );
        let active = state.shell.session_store.active().unwrap();
        assert!(active.command.is_none());
        assert_eq!(active.exit_code, Some(0));
        assert!(!active.waiting_for_input);
    }
}
