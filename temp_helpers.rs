fn handle_paste_tray_key(state: &mut AppState, c: char) -> bool {
    use crate::tui::services::clipboard_paste as cp;
    let len = state.pending_pastes.len();
    match c {
        'j' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_next(state.pending_paste_selected, len);
            true
        }
        'k' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_prev(state.pending_paste_selected, len);
            true
        }
        'J' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_next(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'K' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_prev(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'd' | 'x' => {
            // Remove from the ledger AND strip the placeholder from input text.
            let sel = state.pending_paste_selected.min(len.saturating_sub(1));
            if sel < state.pending_pastes.len() {
                let placeholder = state.pending_pastes[sel].placeholder.clone();
                // input is empty by gate, but be robust if that changes
                if !state.input.is_empty() {
                    let stripped = state.input.get_content().replace(&placeholder, "");
                    state.input.clear();
                    state.input.insert_str(&stripped);
                }
                state.pending_paste_selected = cp::remove_at(&mut state.pending_pastes, sel);
                if state.pending_pastes.is_empty() {
                    state.pending_paste_reorder_mode = false;
                    state.pending_paste_selected = 0;
                }
            }
            true
        }
        'r' => {
            state.pending_paste_reorder_mode = !state.pending_paste_reorder_mode;
            true
        }
        _ => false,
    }
}
fn plan_write_status(state: &mut AppState, new_status: crate::tui::services::plan::PlanStatus) {
    use crate::tui::services::plan;
    let on_disk = std::fs::read_to_string(plan::plan_file_path(&state.project_root)).ok();
    if let Some(disk_content) = on_disk.as_ref() {
        if plan::compute_plan_hash(disk_content) != plan::compute_plan_hash(&state.plan_draft) {
            state.add_assistant_message(
                "Plan file changed on disk since it was loaded. Reload with /plan-review first."
                    .to_string(),
            );
            return;
        }
    }
    let Some(meta) = state.plan_metadata.as_mut() else {
        state.add_assistant_message("No plan metadata loaded.".to_string());
        return;
    };
    meta.status = new_status;
    meta.updated = Some(chrono::Utc::now());
    meta.version = meta.version.saturating_add(1);
    let Ok(fm) = serde_yaml::to_string(meta) else {
        state.add_assistant_message("Failed to serialize plan metadata.".to_string());
        return;
    };
    let body = plan::extract_plan_body(&state.plan_draft).to_string();
    let new_content = format!("---\n{}---\n\n{}", fm, body);
    if let Err(e) = plan::write_plan_file(&state.project_root, &new_content) {
        state.add_assistant_message(format!("Failed to write plan: {}", e));
        return;
    }
    state.plan_draft = new_content;
}
fn plan_open_editor(state: &mut AppState) {
    use crossterm::{
        event::{EnableBracketedPaste, EnableMouseCapture},
        execute,
        terminal::{
            Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
            enable_raw_mode,
        },
    };

    let project_root = state.project_root.clone();
    let plan_path = crate::tui::services::plan::plan_file_path(&project_root);
    if !plan_path.exists() {
        // Seed a minimal template so the editor has something to open.
        let title = state
            .session_title
            .clone()
            .unwrap_or_else(|| "Session Plan".to_string());
        let tmpl = crate::tui::services::plan::new_plan_template(&title);
        if let Err(e) = crate::tui::services::plan::write_plan_file(&project_root, &tmpl) {
            state.add_assistant_message(format!("Failed to create plan: {}", e));
            return;
        }
    }

    let preferred = std::env::var("VAC_EDITOR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .and_then(|s| s.split_whitespace().next().map(|t| t.to_string()));

    let Some(editor) = crate::tui::services::review::detect_editor(preferred) else {
        state.add_assistant_message(
            "No editor available. Set VAC_EDITOR/EDITOR or install nvim/vim/nano.".to_string(),
        );
        return;
    };

    state.push_activity(
        crate::tui::app::ActivityKind::Review,
        format!("Open editor on plan.md: {editor}"),
    );

    state.ask_user_allow_free_text = allow_free_text;
    state.ask_user_question_kind = kind;
    state.ask_user_metadata = metadata;
    state.ask_user_multi_selected.clear();
    state.ask_user_filter.clear();
    state.ask_user_search_active = false;
    state.ask_user_scroll = 0;
    state.ask_user_tool_call_id = Some(tc.id.clone());
    state.show_ask_user_popup = true;
    state.push_activity(
        crate::tui::app::ActivityKind::Approval,
        "Assistant requested input",
    );
}
