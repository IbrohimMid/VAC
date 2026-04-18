//! VacEngine Adapter
//!
//! Bridges the Stakpak TUI shell with VAC's engine/runtime.

use crate::tui::adapter::types::*;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

/// Adapter that bridges TUI events with VacEngine
pub struct VacEngineAdapter {
    /// Receiver for input events from TUI
    input_rx: mpsc::Receiver<AdapterInputEvent>,
    /// Sender for output events to TUI
    output_tx: mpsc::Sender<AdapterOutputEvent>,
    /// Optional engine reference for projection queries
    engine: Option<Arc<RwLock<vac_core::VacEngine>>>,
}

/// Input events from TUI (mapped from Stakpak InputEvent)
#[derive(Debug)]
pub enum AdapterInputEvent {
    /// User sent a message
    UserMessage(String),
    /// Tool call accepted
    AcceptTool(ToolCall),
    /// Tool call rejected
    RejectTool(ToolCall, bool, Option<String>),
    /// Request session list
    ListSessions,
    /// Switch to session
    SwitchToSession(String),
    /// Create new session
    NewSession,
    /// Request model switch
    SwitchToModel(Model),
    /// Request profile switch
    SwitchToProfile(String),
    /// Request rulebook update
    UpdateRulebooks(Vec<String>),
    /// Request TaskGraph inspection
    InspectTaskGraph,
}

/// Output events to TUI (mapped to Stakpak OutputEvent)
#[derive(Debug)]
pub enum AdapterOutputEvent {
    /// Assistant message stream
    AssistantMessage(String),
    /// Stream assistant message chunk
    StreamAssistantMessage(Uuid, String),
    /// Tool result
    ToolResult(ToolCallResult),
    /// Error occurred
    Error(String),
    /// Loading state changed
    LoadingChanged(bool),
    /// Sessions loaded
    SessionsLoaded(Vec<SessionInfo>),
    /// Models loaded
    ModelsLoaded(Vec<Model>),
    /// Usage update
    UsageUpdate(LLMTokenUsage),
    /// TaskGraph info loaded for Inspector UI
    TaskGraphLoaded(vac_runtime::TaskGraph),
    /// TaskGraph projection for runtime inspector
    TaskGraphProjection(vac_core::engine::TaskGraphProjection),
}

/// Session info for TUI
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub checkpoints: Vec<String>,
}

impl VacEngineAdapter {
    pub fn new(
        input_rx: mpsc::Receiver<AdapterInputEvent>,
        output_tx: mpsc::Sender<AdapterOutputEvent>,
    ) -> Self {
        Self {
            input_rx,
            output_tx,
            engine: None,
        }
    }

    pub fn with_engine(mut self, engine: Arc<RwLock<vac_core::VacEngine>>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Process incoming events from TUI and forward to VacEngine
    pub async fn run(&mut self) {
        while let Some(event) = self.input_rx.recv().await {
            match event {
                AdapterInputEvent::UserMessage(msg) => {
                    // Forward to VacEngine when integrated
                    let _ = self
                        .output_tx
                        .send(AdapterOutputEvent::AssistantMessage(format!(
                            "VAC received: {}",
                            msg
                        )))
                        .await;
                }
                AdapterInputEvent::AcceptTool(tc) => {
                    // Forward approval to VacEngine
                    let _ = self
                        .output_tx
                        .send(AdapterOutputEvent::AssistantMessage(format!(
                            "Tool approved: {}",
                            tc.function.name
                        )))
                        .await;
                }
                AdapterInputEvent::RejectTool(tc, _, _) => {
                    let _ = self
                        .output_tx
                        .send(AdapterOutputEvent::AssistantMessage(format!(
                            "Tool rejected: {}",
                            tc.function.name
                        )))
                        .await;
                }
                AdapterInputEvent::ListSessions => {
                    let _ = self
                        .output_tx
                        .send(AdapterOutputEvent::SessionsLoaded(vec![]))
                        .await;
                }
                AdapterInputEvent::InspectTaskGraph => {
                    if let Some(engine_arc) = &self.engine {
                        let guard = engine_arc.read().await;
                        if let Some(proj) = guard.task_graph_projection() {
                            let _ = self
                                .output_tx
                                .send(AdapterOutputEvent::TaskGraphProjection(proj))
                                .await;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
