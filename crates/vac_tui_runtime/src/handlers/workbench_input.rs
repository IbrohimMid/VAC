//! Workbench tab input handlers — dispatched from the main router when
//! `state.focus == WorkspaceFocus::Workbench`.

use crate::app::AppState;
use crate::app::{InputEvent, OutputEvent, WorkbenchTab};
use crate::handlers::input_editor::{plan_open_editor, plan_write_status};
use crate::handlers::{HandlerContext, approval, review as review_handler, vil_workbench};
use tokio::sync::mpsc::Sender;

pub fn handle(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::InputChanged(c) => handle_char(state, output_tx, c),
        InputEvent::Up => handle_up(state, output_tx),
        InputEvent::Down => handle_down(state, output_tx),
        InputEvent::InputSubmitted => handle_submit(state, output_tx),
        _ => {}
    }
}

fn handle_char(state: &mut AppState, output_tx: &Sender<OutputEvent>, c: char) {
    match state.workbench_tab {
        WorkbenchTab::Approvals => match c {
            'a' => {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = approval::approve_current(&mut ctx);
            }
            'r' => {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = approval::begin_reject_current(&mut ctx);
            }
            _ => {}
        },
        WorkbenchTab::Review => {}
        WorkbenchTab::Vil => {
            let mut ctx = HandlerContext::new(state, output_tx);
            match c {
                'r' | 'R' => {
                    let _ = vil_workbench::run_repair(&mut ctx);
                }
                'a' | 'A' => {
                    let _ = vil_workbench::run_audit(&mut ctx);
                }
                'd' | 'D' => {
                    let _ = vil_workbench::run_ir_diff(&mut ctx);
                }
                'o' | 'O' => {
                    let _ = vil_workbench::open_in_editor(&mut ctx);
                }
                'b' | 'B' => {
                    let _ = vil_workbench::run_batch_campaign(&mut ctx);
                }
                _ => {}
            }
        }
        WorkbenchTab::Plan => match c {
            'a' => {
                plan_write_status(state, crate::services::plan::PlanStatus::Approved);
                state.add_assistant_message("Plan approved.".to_string());
            }
            'r' => {
                plan_write_status(state, crate::services::plan::PlanStatus::Drafting);
                state.add_assistant_message("Plan marked for revision.".to_string());
            }
            'e' => plan_open_editor(state),
            _ => {}
        },
        WorkbenchTab::Sessions => {
            if c == 'r' {
                if let Some(sel) = state.sessions.get(state.sessions_selected_idx).cloned() {
                    if sel.has_checkpoint {
                        let _ = output_tx.try_send(OutputEvent::ResumeSession(sel.id.clone()));
                        state.push_activity(
                            crate::app::ActivityKind::Session,
                            format!("Resuming checkpoint: {}", &sel.id[..8.min(sel.id.len())]),
                        );
                    } else {
                        state.toasts.push(crate::services::Toast::info(
                            "No checkpoint available for this session".to_string(),
                        ));
                    }
                }
            } else if c == 'd' {
                cleanup_session(state, output_tx);
            }
        }
        WorkbenchTab::Agents => {
            if c == 'r' {
                let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                let _ = output_tx.try_send(OutputEvent::LoadAgentState);
            }
        }
        WorkbenchTab::Runtime => match c {
            'r' => {
                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
            }
            'c' => {
                if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
                    let _ = output_tx.try_send(OutputEvent::CancelRuntimeJob(job.id));
                }
            }
            't' => {
                if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
                    let _ = output_tx.try_send(OutputEvent::RetryRuntimeJob(job.id));
                }
            }
            _ => {}
        },
    }
}

fn handle_up(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    match state.workbench_tab {
        WorkbenchTab::Approvals => {
            state.approval_selected_idx = state.approval_selected_idx.saturating_sub(1);
            state.approval_detail_scroll = 0;
        }
        WorkbenchTab::Sessions => {
            state.sessions_selected_idx = state.sessions_selected_idx.saturating_sub(1);
        }
        WorkbenchTab::Agents => {
            state.runtime.agent_selected = state.runtime.agent_selected.saturating_sub(1);
            state.runtime.agent_detail_scroll = 0;
        }
        WorkbenchTab::Runtime => {
            state.runtime.selected_idx = state.runtime.selected_idx.saturating_sub(1);
            state.runtime.detail_scroll = 0;
        }
        WorkbenchTab::Review => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::select_prev(&mut ctx);
        }
        WorkbenchTab::Plan => {}
        WorkbenchTab::Vil => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = vil_workbench::select_prev(&mut ctx);
        }
    }
}

fn handle_down(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    match state.workbench_tab {
        WorkbenchTab::Approvals => {
            if state.approval_selected_idx + 1 < state.pending_approvals.len() {
                state.approval_selected_idx += 1;
                state.approval_detail_scroll = 0;
            }
        }
        WorkbenchTab::Sessions => {
            if state.sessions_selected_idx + 1 < state.sessions.len() {
                state.sessions_selected_idx += 1;
            }
        }
        WorkbenchTab::Agents => {
            if state.runtime.agent_selected + 1 < state.runtime.agent_tasks.len() {
                state.runtime.agent_selected += 1;
                state.runtime.agent_detail_scroll = 0;
            }
        }
        WorkbenchTab::Runtime => {
            if state.runtime.selected_idx + 1 < state.runtime.jobs.len() {
                state.runtime.selected_idx += 1;
                state.runtime.detail_scroll = 0;
            }
        }
        WorkbenchTab::Review => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::select_next(&mut ctx);
        }
        WorkbenchTab::Plan => {}
        WorkbenchTab::Vil => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = vil_workbench::select_next(&mut ctx);
        }
    }
}

fn handle_submit(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    match state.workbench_tab {
        WorkbenchTab::Approvals => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_current(&mut ctx);
        }
        WorkbenchTab::Sessions => {
            if let Some(sel) = state.sessions.get(state.sessions_selected_idx).cloned() {
                let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                state.push_activity(crate::app::ActivityKind::Session, "Switch session");
            }
        }
        _ => {}
    }
}

#[allow(deprecated)]
fn cleanup_session(state: &mut AppState, output_tx: &Sender<OutputEvent>) {
    if let Some(sel) = state.sessions.get(state.sessions_selected_idx).cloned() {
        match uuid::Uuid::parse_str(&sel.id) {
            Ok(session_id) => {
                let report = vac_session_control::cleanup_session(&state.project_root, session_id);
                if report.snapshot_removed
                    || report.checkpoint_removed
                    || report.approvals_removed > 0
                {
                    state.toasts.push(crate::services::Toast::success(format!(
                        "Cleaned session artifacts: snapshot {}, checkpoint {}, approvals {}",
                        report.snapshot_removed,
                        report.checkpoint_removed,
                        report.approvals_removed
                    )));
                } else {
                    state.toasts.push(crate::services::Toast::info(
                        "No session artifacts found to clean".to_string(),
                    ));
                }
                if !report.errors.is_empty() {
                    state.toasts.push(crate::services::Toast::error(format!(
                        "Session cleanup completed with {} error(s)",
                        report.errors.len()
                    )));
                }
                let _ = output_tx.try_send(OutputEvent::ListSessions);
            }
            Err(e) => {
                state.toasts.push(crate::services::Toast::error(format!(
                    "Invalid session id: {e}"
                )));
            }
        }
    }
}
