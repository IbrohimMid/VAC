//! Plan-mode input handler.
//!
//! Handles all keyboard events when the plan review overlay is open.
//! Returns `true` if the event was consumed (overlay was active and handled it).

use crate::app::{AppState, InputEvent};

/// Handle a key event when the plan review overlay is visible.
/// Returns `true` if the event was consumed.
pub fn handle_plan_review_key(state: &mut AppState, event: &InputEvent) -> bool {
    if !state.workspace.plan.review_open {
        return false;
    }

    let body = crate::services::plan::extract_plan_body(&state.workspace.plan.draft).to_string();
    let line_count = body.lines().count();

    match event {
        InputEvent::HandleEsc => {
            state.workspace.plan.review_open = false;
        }
        InputEvent::Up | InputEvent::ScrollUp => {
            state.workspace.plan.review_selected =
                state.workspace.plan.review_selected.saturating_sub(1);
            if state.workspace.plan.review_selected < state.workspace.plan.review_scroll {
                state.workspace.plan.review_scroll = state.workspace.plan.review_selected;
            }
        }
        InputEvent::Down | InputEvent::ScrollDown => {
            if line_count > 0 && state.workspace.plan.review_selected + 1 < line_count {
                state.workspace.plan.review_selected += 1;
            }
        }
        InputEvent::PageUp => {
            state.workspace.plan.review_scroll =
                state.workspace.plan.review_scroll.saturating_sub(10);
            state.workspace.plan.review_selected =
                state.workspace.plan.review_selected.saturating_sub(10);
        }
        InputEvent::PageDown => {
            let max_scroll = line_count.saturating_sub(1);
            state.workspace.plan.review_scroll = state
                .workspace
                .plan
                .review_scroll
                .saturating_add(10)
                .min(max_scroll);
            if line_count > 0 {
                state.workspace.plan.review_selected =
                    (state.workspace.plan.review_selected + 10).min(line_count - 1);
            }
        }
        InputEvent::InputChanged('a') => {
            write_plan_status(state, crate::services::plan::PlanStatus::Approved);
            state.workspace.plan.review_open = false;
            state.add_assistant_message("Plan approved.".to_string());
        }
        InputEvent::InputChanged('r') => {
            write_plan_status(state, crate::services::plan::PlanStatus::Drafting);
            state.workspace.plan.review_open = false;
            state.add_assistant_message("Plan marked for revision.".to_string());
        }
        _ => {
            // Trap all other keys so they don't leak into the input behind the overlay.
        }
    }
    true
}

/// Rewrite plan.md with a new status, guarding against concurrent external edits.
pub fn write_plan_status(state: &mut AppState, new_status: crate::services::plan::PlanStatus) {
    use crate::services::plan;
    let on_disk = std::fs::read_to_string(plan::plan_file_path(&state.core.project_root)).ok();
    if let Some(disk_content) = on_disk.as_ref() {
        if plan::compute_plan_hash(disk_content)
            != plan::compute_plan_hash(&state.workspace.plan.draft)
        {
            state.add_assistant_message(
                "Plan file changed on disk since it was loaded. Reload with /plan-review first."
                    .to_string(),
            );
            return;
        }
    }
    let Some(meta) = state.workspace.plan.metadata.as_mut() else {
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
    let body = plan::extract_plan_body(&state.workspace.plan.draft).to_string();
    let new_content = format!("---\n{}---\n\n{}", fm, body);
    if let Err(e) = plan::write_plan_file(&state.core.project_root, &new_content) {
        state.add_assistant_message(format!("Failed to write plan: {}", e));
        return;
    }
    state.workspace.plan.draft = new_content;
}

/// Open plan.md in $EDITOR, suspending the TUI.
pub fn open_editor(state: &mut AppState) {
    use crossterm::{
        event::{EnableBracketedPaste, EnableMouseCapture},
        execute,
        terminal::{
            Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
            enable_raw_mode,
        },
    };

    let project_root = state.core.project_root.clone();
    let plan_path = crate::services::plan::plan_file_path(&project_root);

    // Seed a minimal template if no plan file exists yet.
    if !plan_path.exists() {
        let title = state
            .session
            .session_meta
            .title
            .clone()
            .unwrap_or_else(|| "Session Plan".to_string());
        let tmpl = crate::services::plan::new_plan_template(&title);
        if let Err(e) = crate::services::plan::write_plan_file(&project_root, &tmpl) {
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

    let Some(editor) = crate::services::review::detect_editor(preferred) else {
        state.add_assistant_message(
            "No editor available. Set VAC_EDITOR/EDITOR or install nvim/vim/nano.".to_string(),
        );
        return;
    };

    state.push_activity(
        crate::app::ActivityKind::Review,
        format!("Open editor on plan.md: {editor}"),
    );

    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    let _ = std::process::Command::new(editor).arg(&plan_path).status();
    let _ = execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        Clear(ClearType::All)
    );
    let _ = enable_raw_mode();

    // Reload plan after editor closes.
    if let Some((meta, content)) = crate::services::plan::read_plan_file(&project_root) {
        state.workspace.plan.metadata = Some(meta);
        state.workspace.plan.draft = content;
        state.add_assistant_message("Plan updated from editor.".to_string());
    } else {
        state.add_assistant_message(
            "Plan saved but front matter couldn't be parsed — fix YAML and reload.".to_string(),
        );
    }
}
