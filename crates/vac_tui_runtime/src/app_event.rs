use crate::app::{AppState, OutputEvent};
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VacAppEvent {
    NewSession,
    ClearUi,
    OpenResumePicker,
    ResumeSessionById(String),
    ForkCurrentSession,
    InsertHistoryCell,
    ConsolidateAssistantStream,
    ConsolidateProposedPlan,
    DiffResult,
    RefreshMcpInventory,
    UpdateModel(String),
    ShowInitChecklist,
}

pub fn dispatch_app_event(
    state: &mut AppState,
    _output_tx: &Sender<OutputEvent>,
    event: VacAppEvent,
) {
    match event {
        VacAppEvent::ShowInitChecklist => {
            crate::overlay::open_overlay(state, crate::overlay::OverlayId::InitChecklist);
        }
        VacAppEvent::ClearUi => {
            state.transcript.messages.clear();
            state
                .transcript
                .messages
                .extend(crate::services::helper_block::welcome_messages(None, state));
        }
        VacAppEvent::NewSession => {
            let _ = _output_tx.try_send(OutputEvent::NewSession);
        }
        _ => {
            // Other events are not implemented yet.
        }
    }
}
