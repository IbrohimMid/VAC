//! TUI view rendering orchestration

mod header;
mod input;
mod messages;
mod operator;
mod overlays;
mod pickers;
mod popups;
mod workbench;

use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub use pickers::render_context_chips;

// For event_loop and other usages

// Public functions from popups used elsewhere

/// Main view function — layout orchestrator that calls sub-render functions
pub fn view(f: &mut Frame, state: &mut AppState) {
    if !state.core.hydrated {
        popups::render_boot_skeleton(f, state);
        return;
    }
    let banner_h = crate::services::banner::banner_height(state);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(banner_h),
            Constraint::Min(1),
            Constraint::Length(1), // statusline
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    header::render_header(f, state, chunks[0]);
    if banner_h > 0 {
        crate::services::banner::render_banner(f, chunks[1], state);
    } else {
        state.layout.banner.click_regions.clear();
        state.layout.banner.dismiss_region = None;
    }
    workbench::render_workspace(f, state, chunks[2]);
    crate::services::statusline::render_statusline(f, state, chunks[3]);
    popups::render_footer(f, state, chunks[4]);

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::CommandPalette)
    {
        overlays::render_command_palette(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::Shortcuts)
    {
        overlays::render_shortcuts(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::IsolationSwitcher)
    {
        crate::services::isolation_switcher::render_isolation_switcher(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::ProfileSwitcher)
    {
        crate::services::profile_switcher::render_profile_switcher(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::RulebookSwitcher)
    {
        crate::services::rulebook_switcher::render_rulebook_switcher(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::MessageAction)
    {
        crate::services::message_action_popup::render_message_action_popup(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::ModelSwitcher)
    {
        overlays::render_model_switcher(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::FileSearch)
    {
        overlays::render_file_search(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::Changeset)
    {
        overlays::render_changeset(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::FileChanges)
    {
        crate::services::file_changes_popup::render_file_changes_popup(f, state);
    }

    if state.workspace.plan.review_open {
        crate::services::plan_review::render_plan_review(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::AskUser)
    {
        crate::services::ask_user::render_ask_user_popup(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::Elicitation)
    {
        overlays::render_elicitation(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::ContextInspector)
    {
        overlays::render_context_inspector(f, state);
    }

    if state.execution.shell.session_store.popup_visible {
        popups::render_shell_popup(f, state);
    }

    if !state.layout.toasts.is_empty() {
        overlays::render_toast(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::HelperDropdown)
    {
        let area = f.area();
        let width = (area.width / 2).max(40).min(area.width.saturating_sub(2));
        let count = state.layout.command_palette.filtered_helpers.len().min(5) as u16;
        let height = count + 2; // + borders or arrows
        let x = area.x + 1;
        let y = area.y + area.height.saturating_sub(height + 2); // above footer

        let rect = ratatui::layout::Rect {
            x,
            y,
            width,
            height,
        };
        f.render_widget(ratatui::widgets::Clear, rect);
        crate::services::helper_dropdown::render_file_search_dropdown(f, state, rect);
    } else if state.composer.at_mention.trigger_active && !state.composer.at_mention.results.is_empty() {
        popups::render_at_dropdown(f, state);
    }

    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::FilePicker)
    {
        pickers::render_file_picker(f, state);
    }
    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::TaskTray)
    {
        pickers::render_task_tray(f, state);
    }
    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::ThemePicker)
    {
        pickers::render_theme_picker(f, state);
    }
    if state
        .layout.overlay_manager
        .is_active(crate::overlay::OverlayId::SessionResume)
    {
        pickers::render_session_resume(f, state);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::{AppStateOptions, WorkbenchTab};
    use crate::overlay::{OverlayId, open_overlay};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn render_state_to_string(state: &mut AppState) -> String {
        let backend = TestBackend::new(200, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view(f, state)).unwrap();
        render_to_string(&terminal)
    }

    fn normalized_rendered(rendered: &str) -> String {
        rendered.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn view_smoke_renders() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });

        state.execution.approvals.pending_approvals.push(crate::ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: crate::FunctionCall {
                name: "file_write".to_string(),
                arguments: r#"{\"file_path\":\"src/lib.rs\"}"#.to_string(),
            },
            metadata: None,
        });
        state.layout.switchers.available_models.push(crate::Model {
            id: "kilo-auto/free".to_string(),
            name: "kilo-auto/free".to_string(),
            provider: "anthropic".to_string(),
            supports_reasoning: false,
            ..Default::default()
        });
        open_overlay(&mut state, OverlayId::ModelSwitcher);
        open_overlay(&mut state, OverlayId::FileSearch);
        state.workspace.file_index.search_results = vec!["src/main.rs".to_string()];
        open_overlay(&mut state, OverlayId::Changeset);
        state
            .workspace.changeset_store
            .file_modified("src/main.rs".to_string(), "agent".to_string(), false);
        state.workspace.modified_files = state.workspace.changeset_store.modified_files();

        terminal.draw(|f| view(f, &mut state)).unwrap();
    }

    #[test]
    fn pinned_context_visible_in_view() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        state.core.hydrated = true;
        state.layout.side_panel.visible = true;
        state.layout.pins.files.push("src/lib.rs".to_string());
        state.layout.pins.diagnostics.push("src/main.rs".to_string());

        terminal.draw(|f| view(f, &mut state)).unwrap();
        let rendered = render_to_string(&terminal);
        assert!(rendered.contains("src/lib.rs"));
        assert!(rendered.contains("src/main.rs"));
    }

    #[test]
    fn empty_states_use_explicit_copy() {
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        state.core.hydrated = true;
        state.layout.workbench_tab = WorkbenchTab::Approvals;

        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("no active model selected"));
        assert!(rendered.contains("No pending approvals"));

        state.layout.side_panel.visible = true;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No pinned context yet"));
        assert!(rendered.contains("No sessions loaded yet"));
        assert!(rendered.contains("No MCP servers configured"));

        state.layout.workbench_tab = WorkbenchTab::Sessions;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No sessions loaded yet"));

        state.layout.workbench_tab = WorkbenchTab::Runtime;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No runtime jobs loaded yet"));

        state.layout.workbench_tab = WorkbenchTab::Plan;
        let rendered = render_state_to_string(&mut state);
        let rendered = normalized_rendered(&rendered);
        assert!(rendered.contains("No plan loaded yet"));
    }

    #[test]
    fn boot_shows_skeleton_before_hydration() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        // hydrated starts as false
        assert!(!state.core.hydrated);
        terminal.draw(|f| view(f, &mut state)).unwrap();
        let rendered = render_to_string(&terminal);
        assert!(rendered.contains("Starting"), "boot skeleton must render");
        assert!(
            !rendered.contains("INPUT"),
            "main UI must not render before hydration"
        );
    }

    #[test]
    fn status_never_renders_unknown_placeholder() {
        let mut state = AppState::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });
        state.core.hydrated = true;
        let rendered = render_state_to_string(&mut state);
        assert!(!rendered.contains("vunknown"), "vunknown must not appear");
        assert!(
            !rendered.contains("Model: none"),
            "Model: none must not appear"
        );
    }
}
