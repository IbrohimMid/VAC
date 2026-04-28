//! Shell event surface — narrow event types the bridge emits for the
//! donor UI to consume. Internal VAC `RuntimeUpdate` / `AgentLoopEvent`
//! variants are deliberately NOT re-exported; the mapper in
//! `vac_shell_bridge` translates them into this surface so the donor
//! never depends on VAC's event model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacSubmitRequest {
    pub session_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VacShellEvent {
    AssistantChunk {
        session_id: String,
        text: String,
    },
    AssistantDone {
        session_id: String,
    },

    ApprovalRequested {
        id: String,
        tool_name: String,
        summary: String,
        risk: String,
    },
    ApprovalResolved {
        id: String,
        approved: bool,
    },

    ToolStarted {
        id: String,
        tool_name: String,
        summary: String,
    },
    ToolFinished {
        id: String,
        ok: bool,
        summary: String,
    },

    TaskStarted {
        id: String,
        label: String,
    },
    TaskFinished {
        id: String,
        ok: bool,
        summary: String,
    },

    ShellOutput {
        session_id: String,
        stream: String,
        chunk: String,
    },

    RuntimeStatus {
        label: String,
        value: String,
    },

    Banner {
        level: String,
        text: String,
    },
    Toast {
        level: String,
        text: String,
    },

    AskUser {
        prompt: String,
        options: Vec<String>,
    },
}
