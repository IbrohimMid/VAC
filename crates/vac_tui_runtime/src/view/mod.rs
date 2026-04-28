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
    layout::{Constraint, Direction, Layout, Rect},
};

pub use pickers::render_context_chips;

// For event_loop and other usages

// Public functions from popups used elsewhere

/// Wave 3 #05 — runtime surface.
///
/// Full-width runtime pane with an autopilot header line and the
/// existing workbench Runtime tab body (`RuntimeTab::render`). Keeping
/// the body shared means job-selection, keybindings, and data flow
/// stay identical whether the operator opens runtime as a surface or
/// as a workbench tab; the only visible difference is the real estate.
fn render_runtime_surface(f: &mut Frame, state: &mut AppState, area: Rect) {
    use crate::app::AppState;
    use crate::services::theme::StyleKey;
    use crate::workbench::WorkbenchTabView;
    use ratatui::{text::Span, widgets::Paragraph};

    fn header_line(state: &AppState) -> ratatui::text::Line<'static> {
        let theme = &state.core.theme;
        let muted = theme.style(StyleKey::Muted);
        let accent = theme.style(StyleKey::Accent);
        let ok = theme.style(StyleKey::Success);

        let (state_label, state_style) = match state.execution.runtime.snapshot.as_ref() {
            Some(snap) => {
                let label = format!("{:?}", snap.state).to_lowercase();
                let style = if label == "idle" { ok } else { accent };
                (label, style)
            }
            None => ("idle".to_string(), muted),
        };
        let mode = state
            .execution
            .runtime
            .snapshot
            .as_ref()
            .map(|s| s.mode.clone())
            .unwrap_or_else(|| "monitor-only".to_string());
        let queue_len = state
            .execution
            .runtime
            .snapshot
            .as_ref()
            .map(|s| s.queue_len)
            .unwrap_or(0);
        let running = state
            .execution
            .runtime
            .jobs
            .iter()
            .filter(|j| matches!(j.status, vac_runtime::JobStatus::Running))
            .count();

        ratatui::text::Line::from(vec![
            Span::raw(" "),
            Span::styled("autopilot", accent),
            Span::styled("  ·  ", muted),
            Span::styled(state_label, state_style),
            Span::styled("  ·  ", muted),
            Span::styled(format!("mode {}", mode), muted),
            Span::styled("  ·  ", muted),
            Span::styled(format!("queue {}", queue_len), muted),
            Span::styled("  ·  ", muted),
            Span::styled(format!("running {}", running), muted),
            Span::styled("  ·  ", muted),
            Span::styled("env host", muted),
        ])
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let header = Paragraph::new(header_line(state));
    f.render_widget(header, rows[0]);
    crate::workbench::runtime::RuntimeTab::render(f, state, rows[1]);
}

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
            Constraint::Length(2),
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
    // Wave 3 #05 — top-level surface dispatch.
    match state.layout.surface {
        crate::app::types::Surface::Chat => {
            workbench::render_workspace(f, state, chunks[2]);
        }
        crate::app::types::Surface::Runtime => {
            render_runtime_surface(f, state, chunks[2]);
        }
    }
    crate::services::statusline::render_statusline(f, state, chunks[3]);
    popups::render_footer(f, state, chunks[4]);

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::CommandPalette)
    {
        overlays::render_command_palette(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::Shortcuts)
    {
        overlays::render_shortcuts(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::IsolationSwitcher)
    {
        crate::services::isolation_switcher::render_isolation_switcher(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::ProfileSwitcher)
    {
        crate::services::profile_switcher::render_profile_switcher(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::RulebookSwitcher)
    {
        crate::services::rulebook_switcher::render_rulebook_switcher(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::MessageAction)
    {
        crate::services::message_action_popup::render_message_action_popup(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::ModelSwitcher)
    {
        overlays::render_model_switcher(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::FileSearch)
    {
        overlays::render_file_search(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::Changeset)
    {
        overlays::render_changeset(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::FileChanges)
    {
        crate::services::file_changes_popup::render_file_changes_popup(f, state);
    }

    if state.workspace.plan.review_open {
        crate::services::plan_review::render_plan_review(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::AskUser)
    {
        crate::services::ask_user::render_ask_user_popup(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::Elicitation)
    {
        overlays::render_elicitation(f, state);
    }

    if state
        .layout
        .overlay_manager
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
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::HelperDropdown)
    {
        let area = f.area();
        // Dogfood F2 fix: fit dropdown width to actual content
        // instead of reserving 50% of the frame. Pad column names
        // + descriptions to the widest row, add a small buffer,
        // cap at 60% frame.
        let widest_row = state
            .layout
            .command_palette
            .filtered_helpers
            .iter()
            .map(|h| h.command.chars().count() + h.description.chars().count() + 6)
            .max()
            .unwrap_or(40) as u16;
        let count = state.layout.command_palette.filtered_helpers.len().min(5) as u16;
        // Height = visible items + top/bottom arrow indicators.
        let height = count + 2;

        // Dogfood F2 follow-up: anchor dropdown above the Input
        // pane border (reference: Claude Code's `> /` prompt with
        // dropdown directly above). Pre-fix used a frame-bottom
        // offset which placed the dropdown over the conversation
        // body on wide terminals. Fallback to frame-bottom calc
        // when `input_area` hasn't been populated (first frame).
        let (x, y, width) = if let Some(input_rect) = state.layout.input_area {
            let width = widest_row
                .max(40)
                .min(input_rect.width)
                .min(area.width.saturating_sub(2));
            let x = input_rect.x;
            // Sit one row above the input border; clamp to 0.
            let y = input_rect.y.saturating_sub(height);
            (x, y, width)
        } else {
            let width = widest_row
                .max(40)
                .min((area.width as f32 * 0.6) as u16)
                .min(area.width.saturating_sub(2));
            let x = area.x + 1;
            let y = area.y + area.height.saturating_sub(height + 3);
            (x, y, width)
        };

        let rect = ratatui::layout::Rect {
            x,
            y,
            width,
            height,
        };
        f.render_widget(ratatui::widgets::Clear, rect);
        crate::services::helper_dropdown::render_file_search_dropdown(f, state, rect);
    } else if state.composer.at_mention.trigger_active
        && !state.composer.at_mention.results.is_empty()
    {
        popups::render_at_dropdown(f, state);
    }

    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::FilePicker)
    {
        pickers::render_file_picker(f, state);
    }
    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::TaskTray)
    {
        pickers::render_task_tray(f, state);
    }
    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::ThemePicker)
    {
        pickers::render_theme_picker(f, state);
    }
    if state
        .layout
        .overlay_manager
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

        state
            .execution
            .approvals
            .pending_approvals
            .push(crate::ToolCall {
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
        state.workspace.changeset_store.file_modified(
            "src/main.rs".to_string(),
            "agent".to_string(),
            false,
        );
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
        state
            .layout
            .pins
            .diagnostics
            .push("src/main.rs".to_string());

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
