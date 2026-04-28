//! D18 — init checklist projection.

use vac_shell_contracts::{InitChecklistAction, InitChecklistStatus, InitChecklistViewModel};
use vac_shell_host_doctor::DoctorCheckStatus;

pub struct InitChecklistInputs {
    pub active_model_label: Option<String>,
    pub sessions_count: usize,
    pub approvals_count: usize,
    pub doctor_status: DoctorCheckStatus,
}

pub fn project_init_checklist(inputs: InitChecklistInputs) -> InitChecklistViewModel {
    let mut rows = vec![];

    let (model_status, model_summary) = if inputs.active_model_label.is_some() {
        (
            InitChecklistStatus::Ready,
            format!("model: {}", inputs.active_model_label.as_ref().unwrap()),
        )
    } else {
        (
            InitChecklistStatus::Warning,
            "model not selected".to_string(),
        )
    };
    rows.push(vac_shell_contracts::InitChecklistRow {
        id: "model".to_string(),
        label: "Model".to_string(),
        status: model_status,
        summary: model_summary,
        detail: if inputs.active_model_label.is_some() {
            Some("Use /model to select a provider and model".to_string())
        } else {
            Some("No active model configured. Run /model to select one.".to_string())
        },
        action: InitChecklistAction::OpenModelSwitcher,
    });

    let sessions_status = if inputs.sessions_count > 0 {
        InitChecklistStatus::Ready
    } else {
        InitChecklistStatus::Unknown
    };
    let sessions_summary = format!("sessions: {} total", inputs.sessions_count);
    rows.push(vac_shell_contracts::InitChecklistRow {
        id: "sessions".to_string(),
        label: "Sessions".to_string(),
        status: sessions_status,
        summary: sessions_summary,
        detail: Some("Use /sessions to browse past sessions".to_string()),
        action: InitChecklistAction::OpenSessions,
    });

    let approvals_status = if inputs.approvals_count > 0 {
        InitChecklistStatus::Warning
    } else {
        InitChecklistStatus::Ready
    };
    let approvals_summary = if inputs.approvals_count > 0 {
        format!("{} pending approvals", inputs.approvals_count)
    } else {
        "no pending approvals".to_string()
    };
    rows.push(vac_shell_contracts::InitChecklistRow {
        id: "approvals".to_string(),
        label: "Approvals".to_string(),
        status: approvals_status,
        summary: approvals_summary,
        detail: if inputs.approvals_count > 0 {
            Some("Review and approve pending tool calls".to_string())
        } else {
            Some("No pending approvals".to_string())
        },
        action: InitChecklistAction::None,
    });

    let doctor_status = match inputs.doctor_status {
        DoctorCheckStatus::Ok => InitChecklistStatus::Ready,
        DoctorCheckStatus::Warning | DoctorCheckStatus::Skipped => InitChecklistStatus::Warning,
        DoctorCheckStatus::Error => InitChecklistStatus::Blocked,
    };
    let doctor_summary = format!("doctor: {:?}", inputs.doctor_status);
    let (doctor_action, doctor_detail) = match inputs.doctor_status {
        DoctorCheckStatus::Ok => (
            InitChecklistAction::None,
            Some("All checks passed".to_string()),
        ),
        DoctorCheckStatus::Warning => (
            InitChecklistAction::OpenDoctor,
            Some("Some checks have warnings. Run /doctor for details.".to_string()),
        ),
        DoctorCheckStatus::Skipped => (
            InitChecklistAction::OpenDoctor,
            Some("Some checks were skipped. Run /doctor for details.".to_string()),
        ),
        DoctorCheckStatus::Error => (
            InitChecklistAction::OpenDoctor,
            Some("Critical checks failed. Run /doctor to see what's wrong.".to_string()),
        ),
    };
    rows.push(vac_shell_contracts::InitChecklistRow {
        id: "doctor".to_string(),
        label: "Doctor".to_string(),
        status: doctor_status,
        summary: doctor_summary,
        detail: doctor_detail,
        action: doctor_action,
    });

    let next_action = if inputs.active_model_label.is_none() {
        "select a model with /model".to_string()
    } else if inputs.approvals_count > 0 {
        "review pending approvals".to_string()
    } else if matches!(inputs.doctor_status, DoctorCheckStatus::Error) {
        "run /doctor to fix critical issues".to_string()
    } else {
        "ready to start".to_string()
    };

    InitChecklistViewModel {
        title: "Init Checklist".to_string(),
        rows,
        next_action: Some(next_action),
    }
}
