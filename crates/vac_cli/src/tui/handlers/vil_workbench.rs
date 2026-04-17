//! Handlers for the VIL Issue Workstation tab (Wave 4.1, Unit 9).
//!
//! These handlers operate on the tabs-within-tab "VIL Issues" pane and
//! dispatch VIL tool invocations (`vil_repair`, `vil_audit`, `vil_ir_diff`)
//! via `OutputEvent::InvokeVilTool`, matching the pattern established in
//! [`crate::tui::handlers::message_action::invoke_tool`].

use super::{HandlerContext, HandlerResult};
use crate::tui::app::{ActivityKind, OutputEvent, VilIssueKind, WorkbenchTab, WorkspaceFocus};
use crate::tui::services::Toast;
use crate::tui::services::vil_workbench;

/// Open the VIL Issues workstation tab (also jumps focus to the workbench).
pub fn open(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.focus = WorkspaceFocus::Workbench;
    ctx.state.workbench_tab = WorkbenchTab::Vil;
    ctx.state
        .push_activity(ActivityKind::Status, "Opened VIL Issues workstation");
    Ok(())
}

/// Select next issue within the active filter.
pub fn select_next(ctx: &mut HandlerContext) -> HandlerResult {
    let issues = vil_workbench::classify_issues(ctx.state);
    let view_len = vil_workbench::filtered(ctx.state, &issues).len();
    if view_len == 0 {
        ctx.state.vil_workbench_selected = 0;
        return Ok(());
    }
    let next = ctx.state.vil_workbench_selected.saturating_add(1);
    ctx.state.vil_workbench_selected = next.min(view_len - 1);
    Ok(())
}

/// Select previous issue within the active filter.
pub fn select_prev(ctx: &mut HandlerContext) -> HandlerResult {
    ctx.state.vil_workbench_selected = ctx.state.vil_workbench_selected.saturating_sub(1);
    Ok(())
}

/// Cycle the group filter to the next kind (left arrow moves back one slot;
/// right arrow moves forward). "All" is represented as `None`.
pub fn cycle_filter(ctx: &mut HandlerContext, forward: bool) -> HandlerResult {
    let order = vil_workbench::KIND_ORDER;
    // Slot order in the top strip: [KIND_ORDER..., None (= All)]
    let total = order.len() + 1;
    let current: usize = match ctx.state.vil_workbench_group_filter {
        Some(kind) => order.iter().position(|k| *k == kind).unwrap_or(total - 1),
        None => total - 1,
    };
    let next = if forward {
        (current + 1) % total
    } else {
        (current + total - 1) % total
    };
    ctx.state.vil_workbench_group_filter = if next == total - 1 {
        None
    } else {
        Some(order[next])
    };
    ctx.state.vil_workbench_selected = 0;
    Ok(())
}

/// Dispatch `vil_repair` for the selected issue.
pub fn run_repair(ctx: &mut HandlerContext) -> HandlerResult {
    let Some(issue) = vil_workbench::selected_issue(ctx.state) else {
        ctx.state
            .toasts
            .push(Toast::info("No VIL issue selected.".to_string()));
        return Ok(());
    };
    let args = serde_json::json!({
        "file": issue.file.clone().unwrap_or_default(),
        "kind": issue.kind.label(),
        "issue": issue.raw.clone(),
    });
    invoke_tool(ctx, "vil_repair", args);
    Ok(())
}

/// Dispatch `vil_audit` for the selected issue.
pub fn run_audit(ctx: &mut HandlerContext) -> HandlerResult {
    let Some(issue) = vil_workbench::selected_issue(ctx.state) else {
        ctx.state
            .toasts
            .push(Toast::info("No VIL issue selected.".to_string()));
        return Ok(());
    };
    let files: Vec<String> = issue.file.clone().into_iter().collect();
    let pass_filter = match issue.kind {
        VilIssueKind::ZeroCopy => "zero_copy",
        VilIssueKind::Plumbing => "plumbing",
        VilIssueKind::Semantic => "semantic",
        _ => "all",
    };
    let args = serde_json::json!({
        "files": files,
        "pass_filter": pass_filter,
    });
    invoke_tool(ctx, "vil_audit", args);
    Ok(())
}

/// Dispatch `vil_ir_diff` (HEAD vs working tree) for the selected issue's
/// file.
pub fn run_ir_diff(ctx: &mut HandlerContext) -> HandlerResult {
    let Some(issue) = vil_workbench::selected_issue(ctx.state) else {
        ctx.state
            .toasts
            .push(Toast::info("No VIL issue selected.".to_string()));
        return Ok(());
    };
    let args = serde_json::json!({
        "file": issue.file.clone().unwrap_or_default(),
    });
    invoke_tool(ctx, "vil_ir_diff", args);
    Ok(())
}

/// Suspend the TUI and launch the selected issue's file in `$EDITOR`.
///
/// Mirrors [`crate::tui::handlers::review::open_editor`]: we disable raw
/// mode, leave the alternate screen, run the editor synchronously, then
/// restore TUI state.
pub fn open_in_editor(ctx: &mut HandlerContext) -> HandlerResult {
    use crossterm::{
        event::{EnableBracketedPaste, EnableMouseCapture},
        execute,
        terminal::{
            Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
            enable_raw_mode,
        },
    };

    let Some(issue) = vil_workbench::selected_issue(ctx.state) else {
        ctx.state
            .toasts
            .push(Toast::info("No VIL issue selected.".to_string()));
        return Ok(());
    };
    let Some(path) = issue.file.clone() else {
        ctx.state.toasts.push(Toast::info(
            "Selected issue has no associated file.".to_string(),
        ));
        return Ok(());
    };

    let preferred = std::env::var("VAC_EDITOR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .and_then(|s| s.split_whitespace().next().map(|t| t.to_string()));

    // `detect_editor` lives under the review service; reuse to avoid
    // duplicating the fallback chain (nvim → vim → nano → vi).
    let Some(editor) = crate::tui::services::review::detect_editor(preferred) else {
        ctx.state.add_assistant_message(
            "No editor available. Set VAC_EDITOR/EDITOR or install nvim/vim/nano.".to_string(),
        );
        return Ok(());
    };

    ctx.state.push_activity(
        ActivityKind::Status,
        format!("VIL workbench: open editor on {path}"),
    );

    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    let _ = std::process::Command::new(editor).arg(&path).status();
    let _ = execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        Clear(ClearType::All)
    );
    let _ = enable_raw_mode();
    Ok(())
}

/// Emit a direct VIL tool invocation (no LLM round-trip).
fn invoke_tool(ctx: &mut HandlerContext, tool_name: &str, args: serde_json::Value) {
    ctx.state
        .add_user_message(format!("[Invoking {}]", tool_name));
    let _ = ctx
        .output_tx
        .try_send(OutputEvent::InvokeVilTool(tool_name.to_string(), args));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{AppState, AppStateOptions, OutputEvent};
    use tokio::sync::mpsc;

    fn make_ctx() -> (
        AppState,
        mpsc::Sender<OutputEvent>,
        mpsc::Receiver<OutputEvent>,
    ) {
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap_or_default(),
        });
        state.vil_status.validation_issues = vec![
            "Handler 'create_user' param 'body' contains owned-bytes type 'Vec<u8>' on Network boundary — zero-copy violation".into(),
            "Struct 'Response' manually implements 'VilMessage' — remove plumbing".into(),
            "Struct 'Event' has no VIL role macro — add #[vil_state]".into(),
            "IR drift detected between HEAD and working tree on src/lib.rs".into(),
            "Canonical term violation: use 'changeset' not 'diff-set'".into(),
        ];
        let (tx, rx) = mpsc::channel(16);
        (state, tx, rx)
    }

    #[test]
    fn open_sets_tab_and_focus() {
        let (mut state, tx, _rx) = make_ctx();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        assert!(open(&mut ctx).is_ok());
        assert_eq!(
            ctx.state.workbench_tab,
            crate::tui::app::WorkbenchTab::Vil
        );
        assert_eq!(ctx.state.focus, crate::tui::app::WorkspaceFocus::Workbench);
    }

    #[test]
    fn cycle_filter_forward_and_back_changes_group_filter() {
        let (mut state, tx, _rx) = make_ctx();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        // Start at `None` ("All"). Forward wraps to the first kind.
        assert_eq!(ctx.state.vil_workbench_group_filter, None);
        assert!(cycle_filter(&mut ctx, true).is_ok());
        assert_eq!(
            ctx.state.vil_workbench_group_filter,
            Some(vil_workbench::KIND_ORDER[0])
        );
        // Back from first kind goes to `None` (All).
        assert!(cycle_filter(&mut ctx, false).is_ok());
        assert_eq!(ctx.state.vil_workbench_group_filter, None);
        // Back once more wraps around to the last concrete kind.
        assert!(cycle_filter(&mut ctx, false).is_ok());
        assert_eq!(
            ctx.state.vil_workbench_group_filter,
            Some(*vil_workbench::KIND_ORDER.last().unwrap())
        );
    }

    #[test]
    fn select_next_respects_filter_bounds() {
        let (mut state, tx, _rx) = make_ctx();
        state.vil_workbench_group_filter = Some(VilIssueKind::ZeroCopy);
        let mut ctx = HandlerContext::new(&mut state, &tx);
        // Only 1 zero-copy issue in the fixture — select_next clamps at 0.
        assert!(select_next(&mut ctx).is_ok());
        assert_eq!(ctx.state.vil_workbench_selected, 0);
        assert!(select_next(&mut ctx).is_ok());
        assert_eq!(ctx.state.vil_workbench_selected, 0);
    }

    #[test]
    fn run_repair_emits_invoke_vil_tool_with_repair_name() {
        let (mut state, tx, mut rx) = make_ctx();
        // Select the zero-copy issue (index 0 with ZeroCopy filter).
        state.vil_workbench_group_filter = Some(VilIssueKind::ZeroCopy);
        state.vil_workbench_selected = 0;
        let mut ctx = HandlerContext::new(&mut state, &tx);
        assert!(run_repair(&mut ctx).is_ok());

        // Drain channel: expect exactly one InvokeVilTool event.
        let event = rx.try_recv().expect("expected InvokeVilTool");
        match event {
            OutputEvent::InvokeVilTool(name, args) => {
                assert_eq!(name, "vil_repair");
                assert_eq!(args["kind"], "ZeroCopy");
                assert_eq!(args["file"], "create_user");
                assert!(args["issue"].as_str().unwrap().contains("zero-copy"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_audit_emits_vil_audit_with_pass_filter() {
        let (mut state, tx, mut rx) = make_ctx();
        state.vil_workbench_group_filter = Some(VilIssueKind::ZeroCopy);
        state.vil_workbench_selected = 0;
        let mut ctx = HandlerContext::new(&mut state, &tx);
        assert!(run_audit(&mut ctx).is_ok());
        let event = rx.try_recv().expect("expected InvokeVilTool");
        match event {
            OutputEvent::InvokeVilTool(name, args) => {
                assert_eq!(name, "vil_audit");
                assert_eq!(args["pass_filter"], "zero_copy");
                assert!(args["files"].is_array());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_ir_diff_emits_vil_ir_diff() {
        let (mut state, tx, mut rx) = make_ctx();
        state.vil_workbench_group_filter = Some(VilIssueKind::IrDrift);
        state.vil_workbench_selected = 0;
        let mut ctx = HandlerContext::new(&mut state, &tx);
        assert!(run_ir_diff(&mut ctx).is_ok());
        let event = rx.try_recv().expect("expected InvokeVilTool");
        match event {
            OutputEvent::InvokeVilTool(name, _args) => {
                assert_eq!(name, "vil_ir_diff");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn run_repair_without_selection_emits_nothing_and_toasts() {
        let (mut state, tx, mut rx) = make_ctx();
        state.vil_status.validation_issues.clear();
        let before = state.toasts.len();
        let mut ctx = HandlerContext::new(&mut state, &tx);
        assert!(run_repair(&mut ctx).is_ok());
        assert!(rx.try_recv().is_err());
        assert!(ctx.state.toasts.len() > before);
    }
}
