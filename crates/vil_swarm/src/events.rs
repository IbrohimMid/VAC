//! Structured runtime event envelope for observability.
//! Additive — existing AgentLoopEvent is unchanged.
//! These events are for internal tracking, future OTel export, and audit.

use std::time::Instant;

/// Structured event envelope for agent loop observability.
/// Additive alongside AgentLoopEvent — does not replace it.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// LLM request started.
    LlmRequestStarted {
        message_count: usize,
        estimated_tokens: u64,
    },
    /// LLM response received.
    LlmResponseReceived {
        tokens_used: u64,
        finish_reason: String,
    },
    /// Tool call started.
    ToolStarted {
        id: String,
        name: String,
        lane: String, // "data" | "control"
    },
    /// Tool call completed.
    ToolFinished {
        id: String,
        name: String,
        success: bool,
    },
    /// Hook denied a tool call.
    HookDenied { tool_name: String, reason: String },
    /// Approval required for a tool call.
    ApprovalRequired { tool_name: String, summary: String },
    /// Context budget reduction was applied.
    ContextReduced {
        messages_before: usize,
        messages_after: usize,
        trim_boundary: usize,
    },
    /// Agent loop iteration started.
    IterationStarted { iteration: usize },
    /// Agent loop completed.
    LoopCompleted {
        total_iterations: usize,
        total_tokens: u64,
    },
}

/// Lightweight event collector — accumulates events for a single agent run.
/// Does not block; caller decides what to do with events.
#[derive(Debug, Default)]
pub struct EventCollector {
    pub events: Vec<AgentEvent>,
    pub start: Option<Instant>,
}

impl EventCollector {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            start: Some(Instant::now()),
        }
    }

    pub fn push(&mut self, event: AgentEvent) {
        self.events.push(event);
    }

    pub fn context_reduced(&mut self, before: usize, after: usize, boundary: usize) {
        self.push(AgentEvent::ContextReduced {
            messages_before: before,
            messages_after: after,
            trim_boundary: boundary,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_accumulates_events() {
        let mut c = EventCollector::new();
        c.push(AgentEvent::IterationStarted { iteration: 1 });
        c.context_reduced(10, 6, 4);
        assert_eq!(c.events.len(), 2);
    }

    #[test]
    fn context_reduced_event_fields() {
        let mut c = EventCollector::new();
        c.context_reduced(20, 12, 8);
        match &c.events[0] {
            AgentEvent::ContextReduced {
                messages_before,
                messages_after,
                trim_boundary,
            } => {
                assert_eq!(*messages_before, 20);
                assert_eq!(*messages_after, 12);
                assert_eq!(*trim_boundary, 8);
            }
            _ => panic!("wrong event"),
        }
    }
}
