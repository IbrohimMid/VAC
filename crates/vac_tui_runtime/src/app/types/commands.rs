//! Command and user message types.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum CommandSource {
    /// Slash command with a real TUI handler — does not send a user message.
    BuiltIn,
    /// Slash command that prepends a canned prompt then sends to the agent.
    BuiltInWithPrompt { prompt_content: String },
    /// User-defined command with a prompt template.
    Custom { prompt_content: String },
    /// No TUI handler — the command text is forwarded verbatim to the agent.
    Passthrough,
}

/// Canonical specification for a single slash command.
///
/// This is the single source of truth used by the command palette, shortcuts
/// popup, footer help text, and the slash-command dispatcher.
#[derive(Debug, Clone)]
pub struct HelperCommand {
    pub command: String,
    pub description: String,
    pub source: CommandSource,
    /// Display hint for an associated keyboard shortcut (empty = none).
    pub shortcut: Option<String>,
    /// Whether this command is actually wired to real functionality.
    /// `false` = it will be forwarded as-is and may not do anything useful.
    pub wired: bool,
    /// Where/how this command is visible in TUI operator surfaces.
    pub surface: crate::services::commands::CommandSurface,
}

#[derive(Debug, Clone)]
pub struct PendingUserMessage {
    pub final_input: String,
    pub shell_tool_calls: Option<Vec<crate::types::ToolCallResult>>,
    pub image_parts: Vec<crate::types::ContentPart>,
    pub user_message_text: String,
}

impl PendingUserMessage {
    pub fn new(
        final_input: String,
        shell_tool_calls: Option<Vec<crate::types::ToolCallResult>>,
        image_parts: Vec<crate::types::ContentPart>,
        user_message_text: String,
    ) -> Self {
        Self {
            final_input,
            shell_tool_calls,
            image_parts,
            user_message_text,
        }
    }

    pub fn merge_from(&mut self, next: Self) {
        if !self.final_input.is_empty() && !next.final_input.is_empty() {
            self.final_input.push_str("\n\n");
        }
        self.final_input.push_str(&next.final_input);

        if !self.user_message_text.is_empty() && !next.user_message_text.is_empty() {
            self.user_message_text.push_str("\n\n");
        }
        self.user_message_text.push_str(&next.user_message_text);

        if let Some(calls) = next.shell_tool_calls {
            let mut current = self.shell_tool_calls.take().unwrap_or_default();
            current.extend(calls);
            self.shell_tool_calls = Some(current);
        }

        self.image_parts.extend(next.image_parts);
    }
}

#[derive(Debug, Clone)]
pub struct ExistingPlanPrompt {
    pub inline_prompt: Option<String>,
}

/// Inline comment attached to a plan line.
#[derive(Debug, Clone)]
pub struct PlanComment {
    pub id: String,
    pub line: usize,
    pub author: String,
    pub text: String,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}
