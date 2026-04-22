//! `AppState` constructor, `Default` impl, and helper methods.

use chrono::Utc;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use uuid::Uuid;

use crate::services::textarea::TextArea;
use crate::types::*;

use super::{
    ActivityItem, ActivityKind, AppState, ApprovalsState, FilePickerState, HelperCommand, LoadingStateManager, VilDevState,
    Message, QueueMetrics, RenderMetrics, ReviewItem, ReviewItemStatus, ReviewState, RuntimeState,
    ShellState, ShortcutsPopupMode, StartupSnapshot, TokenUsage, VilLogEntry, VilState,
    WorkbenchTab, WorkspaceFocus,
};

/// Options for creating AppState
pub struct AppStateOptions {
    pub model: Option<Model>,
    pub session_id: Option<String>,
    pub checkpoint_path: Option<PathBuf>,
    pub project_root: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(AppStateOptions {
            model: None,
            session_id: None,
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap_or_default(),
        })
    }
}

impl AppState {
    pub fn new(options: AppStateOptions) -> Self {
        Self {
            startup: StartupSnapshot::default(),
            hydrated: false,
            hydration_deadline: std::time::Instant::now() + std::time::Duration::from_secs(10),
            side_panel_visible: false,
            side_panel_width: 30,
            side_panel_section_collapsed: std::collections::HashSet::new(),
            side_panel_header_areas: std::collections::HashMap::new(),
            side_panel_row_areas: vec![],
            input: TextArea::new(),
            cursor_position: 0,
            focus: WorkspaceFocus::Input,
            messages: Vec::new(),
            scroll: 0,
            loading: false,
            loading_manager: LoadingStateManager::new(),
            spinner_frame: 0,
            session_id: options
                .session_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            sessions: Vec::new(),
            session_title: None,
            checkpoint_path: options.checkpoint_path,
            current_model: options.model,
            mouse_capture_enabled: true,
            approvals: ApprovalsState::default(),
            shell: ShellState::default(),
            is_streaming: false,
            cancel_requested: false,
            quit_press_count: 0,
            quit_first_press: None,
            streaming_message_id: None,
            streaming_start: None,
            streaming_tokens: 0,
            command_palette_input: String::new(),
            command_palette_selected: 0,
            command_palette_scroll: 0,
            commands: Self::default_commands(),
            helper_scroll: 0,
            helper_selected: 0,
            filtered_helpers: Vec::new(),
            recent_commands: crate::services::recent_commands::RecentCommands::load(),

            shortcuts_mode: ShortcutsPopupMode::default(),
            shortcuts_scroll: 0,
            isolation_switcher_selected: 0,
            isolation_modes: vec![
                "host".to_string(),
                "isolated".to_string(),
                "isolated (Rust)".to_string(),
                "isolated (Node)".to_string(),
                "isolated (Python)".to_string(),
            ],
            active_isolation_mode: "host".to_string(),
            profile_switcher_selected: 0,
            profile_search_input: String::new(),
            available_profiles: Vec::new(),
            filtered_profiles: Vec::new(),
            active_profile: "default".to_string(),
            rulebook_switcher_selected: 0,
            rulebook_search_input: String::new(),
            available_rulebooks: Vec::new(),
            filtered_rulebooks: Vec::new(),
            selected_rulebooks: std::collections::HashSet::new(),
            message_action_popup_selected: 0,
            message_action_target_id: None,
            changeset_store: vac_changeset::ChangesetStore::new(),
            modified_files: Vec::new(),
            workbench_tab: WorkbenchTab::Approvals,
            review: ReviewState::default(),
            sessions_selected_idx: 0,
            runtime: RuntimeState::default(),
            activity: Vec::new(),
            activity_scroll: 0,
            toasts: Vec::new(),
            available_models: Vec::new(),
            model_switcher_filter: String::new(),
            model_switcher_selected_idx: 0,
            all_files: Vec::new(),
            file_search_query: String::new(),
            file_search_selected_idx: 0,
            file_search_results: Vec::new(),
            file_picker: FilePickerState::new(options.project_root.clone()),
            context_chips: Vec::new(),
            context_chip_cursor: None,
            at_trigger_active: false,
            at_query: String::new(),
            at_results: Vec::new(),
            at_selected_idx: 0,
            changeset_selected_idx: 0,
            changeset_diff_scroll: 0,
            changeset_selected_path: None,
            changeset_diff: None,
            auto_approve: false,
            project_root: options.project_root,
            mcp_server_states: HashMap::new(),
            vil: VilState::default(),
            vwfd_inspector: crate::services::vwfd_inspector::VwfdInspectorState::default(),
            vil_expr_lint: crate::services::vil_expr_lint::LintState::new(),
            pending_image_parts: vec![],
            banner_message: None,
            banner_click_regions: Vec::new(),
            banner_dismiss_region: None,
            workbench_tab_regions: Vec::new(),
            task_tray_row_regions: Vec::new(),
            review_file_row_regions: Vec::new(),
            pending_kitty_emission: None,
            last_kitty_emission: None,
            image_preview_cache: crate::services::image_preview_cache::ImagePreviewCache::new(),
            approvals_row_regions: Vec::new(),
            vil_issue_row_regions: Vec::new(),
            sessions_row_regions: Vec::new(),
            workbench_body_region: None,
            banner_queue: crate::services::banner::BannerQueue::new(),
            pending_pastes: Vec::new(),
            is_pasting: false,
            paste_counter: 0,
            file_changes_selected: 0,
            file_changes_search: String::new(),
            file_changes_scroll: 0,
            todos: Vec::new(),
            current_message_usage: TokenUsage::default(),
            total_session_usage: TokenUsage::default(),
            context_usage_percent: 0.0,
            billing_info: None,
            auth_display_info: (None, None, None),
            line_to_message_map: Vec::new(),
            pending_revert_index: None,
            plan: super::PlanState::default(),
            ask_user_question: None,
            ask_user_options: Vec::new(),
            ask_user_selected: 0,
            ask_user_input: String::new(),
            ask_user_tool_call_id: None,
            ask_user_allow_free_text: true,
            ask_user_question_kind: crate::services::ask_user::AskUserQuestionKind::SingleSelect,
            ask_user_multi_selected: std::collections::HashSet::new(),
            ask_user_metadata: std::collections::HashMap::new(),
            ask_user_filter: String::new(),
            ask_user_search_active: false,
            ask_user_scroll: 0,
            selection_state: crate::services::text_selection::SelectionState::default(),
            per_message_cache: HashMap::new(),
            render_metrics: RenderMetrics::default(),
            assembled_lines_cache: None,
            collapsed_message_lines_cache: None,
            message_area_y: 0,
            message_area_height: 0,
            input_tx: None,
            overlay_manager: crate::overlay::OverlayManager::new(),
            task_tray_selected: 0,
            task_tray_scroll: 0,
            task_tray_filter_active_only: false,
            theme: crate::services::theme::Theme::default(),
            theme_picker_selected: 0,
            session_resume_query: String::new(),
            session_resume_selected: 0,
            session_resume_list: Vec::new(),
            session_resume_filtered_indices: Vec::new(),
            session_resume_date_filter_days: None,
            // Unit 9 (Wave 4.1) — VIL Issue Workstation
            // vil workbench fields are in vil: VilState::default()
            // Unit 5 (Wave 3.1) — Attachment tray preview & reorder
            pending_paste_selected: 0,
            pending_paste_reorder_mode: false,
            // Context Composer
            context_composer_visible: false,
            validation_score: None,
            validation_issues: Vec::new(),
            lsp_available: false,
            lsp_diagnostics: None,
            diagnostics_overlay_cache:
                crate::services::diagnostics_overlay::DiagnosticsOverlayCache::default(),
            active_hover: None,
            hover_popup_region: None,
            active_hover_row_idx: None,
            pinned_files: Vec::new(),
            pinned_diffs: Vec::new(),
            pinned_diagnostics: Vec::new(),
            pinned_runtime_items: Vec::new(),
            pinned_plan_items: Vec::new(),
            pending_user_messages: VecDeque::new(),
            queue_metrics: QueueMetrics::default(),
            // T14: vil dev runner state
            vil_dev: VilDevState::default(),
        }
    }

    fn default_commands() -> Vec<HelperCommand> {
        crate::services::helper_block::vac_commands()
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::user(content, None));
    }

    /// Count of user-role messages in the current transcript. Computed from
    /// `messages` rather than denormalized so revert/reset operations don't
    /// need to remember to adjust a counter.
    pub fn user_message_count(&self) -> usize {
        self.messages.iter().filter(|m| m.role == "user").count()
    }

    /// Replace any pasted-content placeholder tokens in `raw` with the real
    /// content (for text pastes) or strip them (for image pastes — the image
    /// rides via `pending_image_parts`). Drains `pending_pastes` regardless of
    /// whether every placeholder was found, so the ledger stays in sync with
    /// a submission.
    pub fn expand_pending_pastes(&mut self, raw: &str) -> String {
        use crate::services::clipboard_paste::PastedKind;
        if self.pending_pastes.is_empty() {
            return raw.to_string();
        }
        let mut out = raw.to_string();
        for item in self.pending_pastes.drain(..) {
            let replacement = match item.kind {
                PastedKind::Text { content, .. } => content,
                PastedKind::Image { .. } => String::new(),
            };
            out = out.replace(&item.placeholder, &replacement);
        }
        out
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
    }

    pub fn record_vil_score(&mut self, score: f64) {
        let should_push = match self.vil.score_history.last().copied() {
            Some(prev) => (prev - score).abs() > 0.0001,
            None => true,
        };
        if should_push {
            self.vil.score_history.push(score);
            if self.vil.score_history.len() > 60 {
                let drain = self.vil.score_history.len().saturating_sub(60);
                self.vil.score_history.drain(0..drain);
            }
        }
    }

    pub fn push_vil_log(&mut self, message: impl Into<String>) {
        self.vil.event_log.push_back(VilLogEntry {
            at: Utc::now(),
            message: message.into(),
        });
        while self.vil.event_log.len() > 200 {
            self.vil.event_log.pop_front();
        }
    }

    pub fn filtered_commands(&self) -> Vec<HelperCommand> {
        let mut cmds: Vec<_> = if self.command_palette_input.is_empty() {
            self.commands
                .iter()
                .filter(|c| c.surface != crate::services::commands::CommandSurface::Hidden)
                .cloned()
                .collect()
        } else {
            self.commands
                .iter()
                .filter(|c| {
                    c.surface != crate::services::commands::CommandSurface::Hidden
                        && (c
                            .command
                            .to_lowercase()
                            .contains(&self.command_palette_input.to_lowercase())
                            || c.description
                                .to_lowercase()
                                .contains(&self.command_palette_input.to_lowercase()))
                })
                .cloned()
                .collect()
        };

        cmds.sort_by_key(|cmd| {
            let freq = self
                .recent_commands
                .frequencies
                .get(&cmd.command)
                .copied()
                .unwrap_or(0);
            let recent_idx = self
                .recent_commands
                .history
                .iter()
                .position(|h| h == &cmd.command)
                .unwrap_or(usize::MAX);
            (std::cmp::Reverse(freq), recent_idx)
        });

        cmds
    }

    pub fn model_switcher_filtered(&self) -> Vec<Model> {
        let q = self.model_switcher_filter.trim().to_lowercase();
        let mut out = self
            .available_models
            .iter()
            .filter(|m| {
                q.is_empty()
                    || m.name.to_lowercase().contains(&q)
                    || m.provider.to_lowercase().contains(&q)
                    || m.id.to_lowercase().contains(&q)
            })
            .cloned()
            .collect::<Vec<_>>();
        out.sort_by_key(|m| {
            let recent_idx = self
                .recent_commands
                .recent_models
                .iter()
                .position(|r| r == &m.id)
                .unwrap_or(usize::MAX);
            (recent_idx, m.provider.clone(), m.name.clone())
        });
        out
    }

    pub fn profile_switcher_filtered(&self) -> Vec<String> {
        let q = self.profile_search_input.trim().to_lowercase();
        self.available_profiles
            .iter()
            .filter(|p| q.is_empty() || p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn rulebook_switcher_filtered(&self) -> Vec<crate::types::ListRuleBook> {
        let q = self.rulebook_search_input.trim().to_lowercase();
        self.available_rulebooks
            .iter()
            .filter(|r| {
                q.is_empty()
                    || r.id.to_lowercase().contains(&q)
                    || r.name.to_lowercase().contains(&q)
                    || r.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    pub fn review_sync_items(&mut self) {
        let session_id = uuid::Uuid::parse_str(&self.session_id).ok();
        // Use changeset_store as primary source - active entries only
        let active_paths: std::collections::HashSet<String> = self
            .changeset_store
            .active_entries()
            .iter()
            .map(|e| e.path.clone())
            .collect();

        self.review
            .items
            .retain(|k, v| active_paths.contains(k) || v.status != ReviewItemStatus::Pending);

        for entry in self.changeset_store.active_entries() {
            let path = &entry.path;
            let has_snapshot = session_id
                .map(|sid| {
                    crate::services::review::snapshot_path(&self.project_root, sid, path).exists()
                })
                .unwrap_or(false);

            self.review
                .items
                .entry(path.clone())
                .and_modify(|it| {
                    it.has_snapshot = has_snapshot;
                    it.status = ReviewItemStatus::Pending;
                    it.last_error = None;
                })
                .or_insert_with(|| ReviewItem {
                    path: path.clone(),
                    status: ReviewItemStatus::Pending,
                    has_snapshot,
                    last_error: None,
                    dirty_generation: 0,
                });
        }
    }

    pub fn review_filtered_paths(&self) -> Vec<String> {
        let filter = self.review.filter.trim().to_lowercase();
        // Primary source: changeset_store active entries (insertion order preserved)
        let mut ordered: Vec<String> = vec![];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in self.changeset_store.active_entries() {
            if seen.insert(entry.path.clone()) {
                ordered.push(entry.path.clone());
            }
        }

        // Include any review_items not in store (e.g. Restored/Failed still visible)
        let mut extra: Vec<String> = self
            .review
            .items
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        extra.sort();
        ordered.extend(extra);

        ordered
            .into_iter()
            .filter(|p| {
                if filter.is_empty() {
                    true
                } else {
                    p.to_lowercase().contains(&filter)
                }
            })
            .collect()
    }

    pub fn review_normalize_selection(&mut self) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.review.selected_idx = 0;
            self.review.selected_path = None;
            self.review.diff = None;
            return;
        }

        if let Some(path) = self.review.selected_path.clone() {
            if let Some(idx) = paths.iter().position(|p| p == &path) {
                self.review.selected_idx = idx;
                return;
            }
        }

        if self.review.selected_idx >= paths.len() {
            self.review.selected_idx = paths.len() - 1;
        }
        self.review.selected_path = Some(paths[self.review.selected_idx].clone());
    }

    pub fn review_select_by_delta(&mut self, delta: isize) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.review.selected_idx = 0;
            self.review.selected_path = None;
            self.review.diff = None;
            return;
        }

        let len = paths.len() as isize;
        let mut idx = self.review.selected_idx as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.review.selected_idx = idx as usize;
        self.review.selected_path = Some(paths[self.review.selected_idx].clone());
    }

    pub fn approval_normalize_selection(&mut self) {
        if self.approvals.pending_approvals.is_empty() {
            self.approvals.approval_selected_idx = 0;
            self.approval_reset_detail();
            return;
        }
        if self.approvals.approval_selected_idx >= self.approvals.pending_approvals.len() {
            self.approvals.approval_selected_idx = self.approvals.pending_approvals.len() - 1;
            self.approval_reset_detail();
        }
    }

    pub fn push_activity(&mut self, kind: ActivityKind, message: impl Into<String>) {
        self.activity.push(ActivityItem {
            at: Utc::now(),
            kind,
            message: message.into(),
        });
        if self.activity.len() > 500 {
            let overflow = self.activity.len() - 500;
            self.activity.drain(0..overflow);
            self.activity_scroll = self.activity_scroll.saturating_sub(overflow);
        }
    }

    fn approval_reset_detail(&mut self) {
        self.approvals.approval_detail_scroll = 0;
    }

    /// Build a borrowing snapshot registry of every signal buffer owned by
    /// this AppState. Used by MCP retrieval tools, distillation surfaces,
    /// and future trajectory exporters so they don't need per-subsystem
    /// knowledge. Stream IDs follow the `<kind>:<identifier>` convention.
    pub fn signal_registry(&self) -> vac_signal::SignalRegistry<'_> {
        let mut reg = vac_signal::SignalRegistry::new();
        reg.register("vil_dev", &self.vil_dev.output);
        for (idx, session) in self.shell.session_store.sessions.iter().enumerate() {
            reg.register(format!("shell:{idx}:{}", session.id), &session.output_signal);
        }
        reg
    }

    /// Convenience: distilled view of the `vil dev` output stream using
    /// default heuristics. Returns `None` if the buffer is empty.
    pub fn vil_dev_distilled(&self, tail_size: usize) -> Option<vac_signal::DistilledView> {
        if self.vil_dev.output.is_empty() {
            return None;
        }
        Some(self.vil_dev.output.distilled_default(tail_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_registry_includes_vil_dev_and_shell_sessions() {
        let mut state = AppState::default();
        state.vil_dev.output.push_line("boot ok");
        state.shell.session_store.push_new("shell-0".to_string());
        state.shell.session_store.push_new("shell-1".to_string());

        let reg = state.signal_registry();
        let ids: Vec<_> = reg.ids().collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().any(|id| *id == "vil_dev"));
        assert!(ids.iter().filter(|id| id.starts_with("shell:")).count() == 2);
    }

    #[test]
    fn vil_dev_distilled_is_none_when_empty_else_some() {
        let mut state = AppState::default();
        assert!(state.vil_dev_distilled(10).is_none());
        state.vil_dev.output.push_line("Error: something");
        let view = state.vil_dev_distilled(5).expect("present");
        assert!(view.key_lines.iter().any(|l| l.contains("Error")));
    }
}
