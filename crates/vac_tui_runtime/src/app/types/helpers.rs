//! `AppState` constructor, `Default` impl, and helper methods.

use chrono::Utc;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use uuid::Uuid;

use crate::services::textarea::TextArea;
use crate::types::*;

use super::{
    ActivityItem, ActivityKind, AppState, ApprovalsState, AskUserState, AtMentionState, BannerState, ChangesetUiState, CommandPaletteState, FileIndexState, FilePickerState, HelperCommand, LoadingStateManager, LspUiState, MessageUiState, OperatorState, PasteState, PinsState, QuitState, ScrollState, SessionResumeState, SidePanelState, StreamingState, SwitchersState, TaskTrayState, ViewFlagsState, VilDevState, WorkbenchChromeState,
    Message, QueueMetrics, RenderMetrics, ReviewItem, ReviewItemStatus, ReviewState, RuntimeState,
    ShellState, StartupSnapshot, TokenUsage, VilLogEntry, VilState,
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
            core: super::CoreState {
                startup: StartupSnapshot::default(),
                hydrated: false,
                hydration_deadline: std::time::Instant::now() + std::time::Duration::from_secs(10),
                loading: false,
                loading_manager: LoadingStateManager::new(),
                view_flags: ViewFlagsState::default(),
                quit: QuitState::default(),
                input_tx: None,
                project_root: options.project_root.clone(),
                theme: crate::services::theme::Theme::default(),
                render_metrics: RenderMetrics::default(),
            },
            layout: super::LayoutState {
                side_panel: SidePanelState::default(),
                focus: WorkspaceFocus::Input,
                scroll: ScrollState::default(),
                workbench_tab: WorkbenchTab::Approvals,
                workbench_chrome: WorkbenchChromeState::default(),
                command_palette: CommandPaletteState::default(),
                commands: Self::default_commands(),
                banner: BannerState::default(),
                toasts: Vec::new(),
                image_render: super::ImageRenderState::default(),
                paste: PasteState::default(),
                message_ui: MessageUiState::default(),
                overlay_manager: crate::overlay::OverlayManager::new(),
                lsp_ui: LspUiState::default(),
                pins: PinsState::default(),
                switchers: SwitchersState::default(),
                session_resume: SessionResumeState::default(),
                ask_user: AskUserState::default(),
                elicitation: None,
            },
            composer: super::ComposerState {
                input: TextArea::new(),
                context_chips: Vec::new(),
                context_chip_cursor: None,
                at_mention: AtMentionState::default(),
                pending_image_parts: vec![],
                prompt_history: crate::services::prompt_suggest::PromptHistory::default(),
                vil_expr_lint: crate::services::vil_expr_lint::LintState::new(),
                selection_state: crate::services::text_selection::SelectionState::default(),
            },
            transcript: super::TranscriptState {
                messages: Vec::new(),
                pending_user_messages: VecDeque::new(),
                todos: Vec::new(),
                streaming: StreamingState::default(),
                rate_limit: crate::services::rate_limit::RateLimitState::default(),
            },
            session: super::SessionDomainState {
                session_id: options.session_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                sessions: Vec::new(),
                session_meta: super::SessionMetaState {
                    title: None,
                    checkpoint_path: options.checkpoint_path,
                    loading: false,
                },
            },
            workspace: super::WorkspaceState {
                file_index: FileIndexState::default(),
                file_picker: FilePickerState::new(options.project_root),
                modified_files: Vec::new(),
                changeset_store: vac_changeset::ChangesetStore::new(),
                changeset_ui: ChangesetUiState::default(),
                review: ReviewState::default(),
                plan: super::PlanState::default(),
                memory_archive: super::MemoryArchiveCache::default(),
            },
            vil_domain: super::VilDomainState {
                vil: VilState::default(),
                vil_dev: VilDevState::default(),
                vwfd_inspector: crate::services::vwfd_inspector::VwfdInspectorState::default(),
            },
            execution: super::ExecutionState {
                shell: ShellState::default(),
                runtime: RuntimeState::default(),
                bridge: super::BridgeState::default(),
                mcp_maps: super::McpMapsState::default(),
                approvals: ApprovalsState::default(),
                task_tray: TaskTrayState::default(),
                queue_metrics: QueueMetrics::default(),
                activity: Vec::new(),
                policy: None,
                lsp: super::LspSnapshot::default(),
                cron: super::CronSnapshot::default(),
                hook_log: Vec::new(),
            },
            operator_config: super::OperatorConfigState {
                operator: OperatorState {
                    current_model: options.model,
                    ..OperatorState::default()
                },
                billing: super::BillingState::default(),
            },
            team: super::TeamContext::default(),
            speculation: super::SpeculationCache::default(),
        }
    }

    fn default_commands() -> Vec<HelperCommand> {
        crate::services::helper_block::vac_commands()
    }

    pub fn add_user_message(&mut self, content: String) {
        self.transcript.messages.push(Message::user(content, None));
    }

    /// Count of user-role messages in the current transcript. Computed from
    /// `messages` rather than denormalized so revert/reset operations don't
    /// need to remember to adjust a counter.
    pub fn user_message_count(&self) -> usize {
        self.transcript.messages.iter().filter(|m| m.role == "user").count()
    }

    /// Replace any pasted-content placeholder tokens in `raw` with the real
    /// content (for text pastes) or strip them (for image pastes — the image
    /// rides via `pending_image_parts`). Drains `pending_pastes` regardless of
    /// whether every placeholder was found, so the ledger stays in sync with
    /// a submission.
    pub fn expand_pending_pastes(&mut self, raw: &str) -> String {
        use crate::services::clipboard_paste::PastedKind;
        if self.layout.paste.pending_pastes.is_empty() {
            return raw.to_string();
        }
        let mut out = raw.to_string();
        for item in self.layout.paste.pending_pastes.drain(..) {
            let replacement = match item.kind {
                PastedKind::Text { content, .. } => content,
                PastedKind::Image { .. } => String::new(),
            };
            out = out.replace(&item.placeholder, &replacement);
        }
        out
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.transcript.messages.push(Message::assistant(content));
    }

    pub fn record_vil_score(&mut self, score: f64) {
        let should_push = match self.vil_domain.vil.score_history.last().copied() {
            Some(prev) => (prev - score).abs() > 0.0001,
            None => true,
        };
        if should_push {
            self.vil_domain.vil.score_history.push(score);
            if self.vil_domain.vil.score_history.len() > 60 {
                let drain = self.vil_domain.vil.score_history.len().saturating_sub(60);
                self.vil_domain.vil.score_history.drain(0..drain);
            }
        }
    }

    pub fn push_vil_log(&mut self, message: impl Into<String>) {
        self.vil_domain.vil.event_log.push_back(VilLogEntry {
            at: Utc::now(),
            message: message.into(),
        });
        while self.vil_domain.vil.event_log.len() > 200 {
            self.vil_domain.vil.event_log.pop_front();
        }
    }

    pub fn filtered_commands(&self) -> Vec<HelperCommand> {
        let mut cmds: Vec<_> = if self.layout.command_palette.input.is_empty() {
            self.layout.commands
                .iter()
                .filter(|c| c.surface != crate::services::commands::CommandSurface::Hidden)
                .cloned()
                .collect()
        } else {
            self.layout.commands
                .iter()
                .filter(|c| {
                    c.surface != crate::services::commands::CommandSurface::Hidden
                        && (c
                            .command
                            .to_lowercase()
                            .contains(&self.layout.command_palette.input.to_lowercase())
                            || c.description
                                .to_lowercase()
                                .contains(&self.layout.command_palette.input.to_lowercase()))
                })
                .cloned()
                .collect()
        };

        cmds.sort_by_key(|cmd| {
            let freq = self
                .layout.command_palette.recent_commands
                .frequencies
                .get(&cmd.command)
                .copied()
                .unwrap_or(0);
            let recent_idx = self
                .layout.command_palette.recent_commands
                .history
                .iter()
                .position(|h| h == &cmd.command)
                .unwrap_or(usize::MAX);
            (std::cmp::Reverse(freq), recent_idx)
        });

        cmds
    }

    pub fn model_switcher_filtered(&self) -> Vec<Model> {
        let q = self.layout.switchers.model_filter.trim().to_lowercase();
        let mut out = self
            .layout.switchers.available_models
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
                .layout.command_palette.recent_commands
                .recent_models
                .iter()
                .position(|r| r == &m.id)
                .unwrap_or(usize::MAX);
            (recent_idx, m.provider.clone(), m.name.clone())
        });
        out
    }

    pub fn profile_switcher_filtered(&self) -> Vec<String> {
        let q = self.layout.switchers.profile_search.trim().to_lowercase();
        self.layout.switchers.available_profiles
            .iter()
            .filter(|p| q.is_empty() || p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn rulebook_switcher_filtered(&self) -> Vec<crate::types::ListRuleBook> {
        let q = self.layout.switchers.rulebook_search.trim().to_lowercase();
        self.layout.switchers.available_rulebooks
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
        let session_id = uuid::Uuid::parse_str(&self.session.session_id).ok();
        // Use changeset_store as primary source - active entries only
        let active_paths: std::collections::HashSet<String> = self
            .workspace.changeset_store
            .active_entries()
            .iter()
            .map(|e| e.path.clone())
            .collect();

        self.workspace.review
            .items
            .retain(|k, v| active_paths.contains(k) || v.status != ReviewItemStatus::Pending);

        for entry in self.workspace.changeset_store.active_entries() {
            let path = &entry.path;
            let has_snapshot = session_id
                .map(|sid| {
                    crate::services::review::snapshot_path(&self.core.project_root, sid, path).exists()
                })
                .unwrap_or(false);

            self.workspace.review
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
        let filter = self.workspace.review.filter.trim().to_lowercase();
        // Primary source: changeset_store active entries (insertion order preserved)
        let mut ordered: Vec<String> = vec![];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in self.workspace.changeset_store.active_entries() {
            if seen.insert(entry.path.clone()) {
                ordered.push(entry.path.clone());
            }
        }

        // Include any review_items not in store (e.g. Restored/Failed still visible)
        let mut extra: Vec<String> = self
            .workspace.review
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
            self.workspace.review.selected_idx = 0;
            self.workspace.review.selected_path = None;
            self.workspace.review.diff = None;
            return;
        }

        if let Some(path) = self.workspace.review.selected_path.clone() {
            if let Some(idx) = paths.iter().position(|p| p == &path) {
                self.workspace.review.selected_idx = idx;
                return;
            }
        }

        if self.workspace.review.selected_idx >= paths.len() {
            self.workspace.review.selected_idx = paths.len() - 1;
        }
        self.workspace.review.selected_path = Some(paths[self.workspace.review.selected_idx].clone());
    }

    pub fn review_select_by_delta(&mut self, delta: isize) {
        let paths = self.review_filtered_paths();
        if paths.is_empty() {
            self.workspace.review.selected_idx = 0;
            self.workspace.review.selected_path = None;
            self.workspace.review.diff = None;
            return;
        }

        let len = paths.len() as isize;
        let mut idx = self.workspace.review.selected_idx as isize + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= len {
            idx = len - 1;
        }
        self.workspace.review.selected_idx = idx as usize;
        self.workspace.review.selected_path = Some(paths[self.workspace.review.selected_idx].clone());
    }

    pub fn approval_normalize_selection(&mut self) {
        if self.execution.approvals.pending_approvals.is_empty() {
            self.execution.approvals.approval_selected_idx = 0;
            self.approval_reset_detail();
            return;
        }
        if self.execution.approvals.approval_selected_idx >= self.execution.approvals.pending_approvals.len() {
            self.execution.approvals.approval_selected_idx = self.execution.approvals.pending_approvals.len() - 1;
            self.approval_reset_detail();
        }
    }

    pub fn push_activity(&mut self, kind: ActivityKind, message: impl Into<String>) {
        self.execution.activity.push(ActivityItem {
            at: Utc::now(),
            kind,
            message: message.into(),
        });
        if self.execution.activity.len() > 500 {
            let overflow = self.execution.activity.len() - 500;
            self.execution.activity.drain(0..overflow);
            self.layout.scroll.activity = self.layout.scroll.activity.saturating_sub(overflow);
        }
    }

    fn approval_reset_detail(&mut self) {
        self.execution.approvals.approval_detail_scroll = 0;
    }

    /// Build a borrowing snapshot registry of every signal buffer owned by
    /// this AppState. Used by MCP retrieval tools, distillation surfaces,
    /// and future trajectory exporters so they don't need per-subsystem
    /// knowledge. Stream IDs follow the `<kind>:<identifier>` convention.
    pub fn signal_registry(&self) -> vac_signal::SignalRegistry<'_> {
        let mut reg = vac_signal::SignalRegistry::new();
        reg.register("vil_dev", &self.vil_domain.vil_dev.output);
        for (idx, session) in self.execution.shell.session_store.sessions.iter().enumerate() {
            reg.register(format!("shell:{idx}:{}", session.id), &session.output_signal);
        }
        for (name, buf) in &self.execution.mcp_maps.server_signals {
            reg.register(format!("mcp:{name}"), buf);
        }
        for (id, buf) in &self.execution.mcp_maps.runtime_signals {
            reg.register(format!("runtime:{id}"), buf);
        }
        reg
    }

    /// Convenience: distilled view of the `vil dev` output stream using
    /// default heuristics. Returns `None` if the buffer is empty.
    pub fn vil_dev_distilled(&self, tail_size: usize) -> Option<vac_signal::DistilledView> {
        if self.vil_domain.vil_dev.output.is_empty() {
            return None;
        }
        Some(self.vil_domain.vil_dev.output.distilled_default(tail_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_registry_includes_vil_dev_and_shell_sessions() {
        let mut state = AppState::default();
        state.vil_domain.vil_dev.output.push_line("boot ok");
        state.execution.shell.session_store.push_new("shell-0".to_string());
        state.execution.shell.session_store.push_new("shell-1".to_string());

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
        state.vil_domain.vil_dev.output.push_line("Error: something");
        let view = state.vil_dev_distilled(5).expect("present");
        assert!(view.key_lines.iter().any(|l| l.contains("Error")));
    }
}
