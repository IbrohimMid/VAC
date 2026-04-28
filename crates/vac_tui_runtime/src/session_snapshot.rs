//! Session snapshot persistence and restoration.
//!
//! Extracted from event_loop.rs to keep file sizes under 600 lines.

use crate::app::{AppState, SidePanelSection, WorkspaceFocus};

pub(crate) fn focus_to_label(focus: WorkspaceFocus) -> &'static str {
    match focus {
        WorkspaceFocus::Conversation => "conversation",
        WorkspaceFocus::Input => "input",
        WorkspaceFocus::Workbench => "workbench",
        WorkspaceFocus::Activity => "activity",
    }
}

pub(crate) fn focus_from_label(label: &str) -> WorkspaceFocus {
    match label {
        "conversation" => WorkspaceFocus::Conversation,
        "workbench" => WorkspaceFocus::Workbench,
        "activity" => WorkspaceFocus::Activity,
        _ => WorkspaceFocus::Input,
    }
}

pub(crate) fn side_panel_section_to_label(section: SidePanelSection) -> &'static str {
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

pub(crate) fn side_panel_section_from_label(label: &str) -> Option<SidePanelSection> {
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

pub(crate) fn build_session_snapshot(
    state: &AppState,
) -> Option<vac_session_control::SessionSnapshot> {
    let session_id = uuid::Uuid::parse_str(&state.session.session_id).ok()?;
    let mut snapshot =
        vac_session_control::SessionSnapshot::new(session_id, state.core.project_root.clone());
    let todo_pending = state
        .transcript
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::Pending))
        .count();
    let todo_in_progress = state
        .transcript
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::InProgress))
        .count();
    let todo_done = state
        .transcript
        .todos
        .iter()
        .filter(|todo| matches!(todo.status, vac_changeset::TodoStatus::Done))
        .count();

    snapshot.active_model = state
        .operator_config
        .operator
        .current_model
        .as_ref()
        .map(|model| model.name.clone())
        .or_else(|| state.core.startup.active_model.clone());
    snapshot.active_profile = Some(state.layout.switchers.active_profile.clone());
    let mut selected_rulebooks: Vec<String> = state
        .layout
        .switchers
        .selected_rulebooks
        .iter()
        .cloned()
        .collect();
    selected_rulebooks.sort();
    snapshot.active_rulebooks = selected_rulebooks;
    snapshot.task_count = state.transcript.todos.len();
    snapshot.completed_tasks = todo_done;
    snapshot.failed_tasks = 0;
    snapshot.total_tokens = state.operator_config.billing.total_session.total_tokens;
    snapshot.modified_files = state.workspace.modified_files.len();
    snapshot.tui_state.active_tab_idx = Some(crate::workbench::active_tab_index(
        &state.layout.workbench_tab,
    ));
    snapshot.tui_state.history_selection =
        Some(state.operator_config.operator.sessions_selected_idx);
    snapshot.tui_state.last_focus = Some(focus_to_label(state.layout.focus).to_string());
    snapshot.tui_state.collapsed_sections = state
        .layout
        .side_panel
        .section_collapsed
        .iter()
        .map(|section| side_panel_section_to_label(*section).to_string())
        .collect();
    snapshot
        .metadata
        .insert("focus".into(), focus_to_label(state.layout.focus).into());
    snapshot.metadata.insert(
        "active_profile".into(),
        state.layout.switchers.active_profile.clone(),
    );
    snapshot.metadata.insert(
        "active_isolation_mode".into(),
        state.layout.switchers.active_isolation_mode.clone(),
    );
    snapshot.metadata.insert(
        "provider_status".into(),
        state.core.startup.provider_status.clone(),
    );
    snapshot.metadata.insert(
        "pending_approvals".into(),
        state
            .execution
            .approvals
            .pending_approvals
            .len()
            .to_string(),
    );
    snapshot.metadata.insert(
        "queue_depth".into(),
        state.transcript.pending_user_messages.len().to_string(),
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

pub(crate) fn apply_session_snapshot(
    state: &mut AppState,
    snapshot: &vac_session_control::SessionSnapshot,
) {
    state.core.startup.active_model = snapshot.active_model.clone();
    state.core.startup.active_profile = snapshot.active_profile.clone();
    state.core.startup.selected_rulebooks = snapshot.active_rulebooks.clone();
    state.layout.switchers.selected_rulebooks = snapshot.active_rulebooks.iter().cloned().collect();
    state.core.startup.active_rulebook = if snapshot.active_rulebooks.is_empty() {
        None
    } else {
        Some(snapshot.active_rulebooks.join(", "))
    };
    if let Some(idx) = snapshot.tui_state.active_tab_idx {
        state.layout.workbench_tab = crate::workbench::tab_from_index(idx);
    }
    if let Some(selection) = snapshot.tui_state.history_selection {
        state.operator_config.operator.sessions_selected_idx = selection;
    }
    if let Some(focus) = snapshot.tui_state.last_focus.as_deref() {
        state.layout.focus = focus_from_label(focus);
    }
    state.layout.side_panel.section_collapsed = snapshot
        .tui_state
        .collapsed_sections
        .iter()
        .filter_map(|section| side_panel_section_from_label(section))
        .collect();
    state.layout.switchers.active_profile = snapshot
        .active_profile
        .clone()
        .unwrap_or_else(|| "default".to_string());
    if let Some(provider_status) = snapshot.metadata.get("provider_status") {
        state.core.startup.provider_status = provider_status.clone();
    }
}

pub(crate) async fn persist_session_snapshot(state: &AppState) {
    let Some(snapshot) = build_session_snapshot(state) else {
        return;
    };
    let result = vac_session_control::save_snapshot_async(snapshot).await;
    if let Err(err) = result {
        tracing::warn!("failed to persist session snapshot: {err}");
    }
}

pub(crate) async fn load_session_snapshot(
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
