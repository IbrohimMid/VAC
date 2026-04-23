use serde::{Deserialize, Serialize};

/// Hints that tell TUI / transcript / trace surfaces how to render a
/// tool invocation. Consumed by `workbench/approvals.rs`, trace
/// exporter, and bridge companion UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRenderHints {
    /// Operator-visible name (falls back to the tool's identifier).
    pub user_facing_name: Option<String>,
    /// Short one-line category used for grouping in list views
    /// ("VIL", "Shell", "File", "Memory").
    pub category: Option<String>,
    /// Progress rendering class.
    pub progress_kind: ProgressKind,
    /// Whether the tool result should appear in the review pane or
    /// stay hidden unless explicitly expanded.
    pub review_visibility: ReviewVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    /// No progress indicator; tool is fast.
    Instant,
    /// Spinner only.
    Spinner,
    /// Determinate progress (0..=100%).
    Percent,
    /// Streaming token/line output.
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVisibility {
    /// Always shown in the review pane.
    Visible,
    /// Shown only when the row is expanded.
    Collapsed,
    /// Never shown in review (e.g. sleep, send_message).
    Hidden,
}

impl Default for ToolRenderHints {
    fn default() -> Self {
        Self {
            user_facing_name: None,
            category: None,
            progress_kind: ProgressKind::Spinner,
            review_visibility: ReviewVisibility::Collapsed,
        }
    }
}
