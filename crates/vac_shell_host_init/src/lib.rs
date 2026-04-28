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

    rows.push(vac_shell_contracts::InitChecklistRow {
        id: "status".to_string(),
        label: "Status".to_string(),
        status: InitChecklistStatus::Ready,
        summary: "status rows available".to_string(),
        detail: Some("Use /status for full readiness summary".to_string()),
        action: InitChecklistAction::OpenStatus,
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

#[cfg(test)]
mod tests {
    use super::*;
    use vac_shell_contracts::InitChecklistAction;

    fn all_ready_inputs() -> InitChecklistInputs {
        InitChecklistInputs {
            active_model_label: Some("claude-sonnet-4.5".to_string()),
            sessions_count: 3,
            approvals_count: 0,
            doctor_status: DoctorCheckStatus::Ok,
        }
    }

    #[test]
    fn no_active_model_yields_warning_and_open_model_action() {
        let inputs = InitChecklistInputs {
            active_model_label: None,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        let model_row = vm.rows.iter().find(|r| r.id == "model").unwrap();
        assert_eq!(model_row.status, InitChecklistStatus::Warning);
        assert_eq!(model_row.action, InitChecklistAction::OpenModelSwitcher);
    }

    #[test]
    fn active_model_present_yields_ready() {
        let vm = project_init_checklist(all_ready_inputs());
        let model_row = vm.rows.iter().find(|r| r.id == "model").unwrap();
        assert_eq!(model_row.status, InitChecklistStatus::Ready);
    }

    #[test]
    fn zero_sessions_yields_unknown() {
        let inputs = InitChecklistInputs {
            sessions_count: 0,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        let sessions_row = vm.rows.iter().find(|r| r.id == "sessions").unwrap();
        assert_eq!(sessions_row.status, InitChecklistStatus::Unknown);
    }

    #[test]
    fn sessions_present_yields_ready() {
        let vm = project_init_checklist(all_ready_inputs());
        let sessions_row = vm.rows.iter().find(|r| r.id == "sessions").unwrap();
        assert_eq!(sessions_row.status, InitChecklistStatus::Ready);
    }

    #[test]
    fn pending_approvals_yields_warning() {
        let inputs = InitChecklistInputs {
            approvals_count: 2,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        let approvals_row = vm.rows.iter().find(|r| r.id == "approvals").unwrap();
        assert_eq!(approvals_row.status, InitChecklistStatus::Warning);
    }

    #[test]
    fn no_approvals_yields_ready() {
        let vm = project_init_checklist(all_ready_inputs());
        let approvals_row = vm.rows.iter().find(|r| r.id == "approvals").unwrap();
        assert_eq!(approvals_row.status, InitChecklistStatus::Ready);
    }

    #[test]
    fn doctor_error_yields_blocked() {
        let inputs = InitChecklistInputs {
            doctor_status: DoctorCheckStatus::Error,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        let doctor_row = vm.rows.iter().find(|r| r.id == "doctor").unwrap();
        assert_eq!(doctor_row.status, InitChecklistStatus::Blocked);
        assert_eq!(doctor_row.action, InitChecklistAction::OpenDoctor);
    }

    #[test]
    fn doctor_warning_yields_warning() {
        let inputs = InitChecklistInputs {
            doctor_status: DoctorCheckStatus::Warning,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        let doctor_row = vm.rows.iter().find(|r| r.id == "doctor").unwrap();
        assert_eq!(doctor_row.status, InitChecklistStatus::Warning);
        assert_eq!(doctor_row.action, InitChecklistAction::OpenDoctor);
    }

    #[test]
    fn doctor_ok_yields_ready_no_action() {
        let vm = project_init_checklist(all_ready_inputs());
        let doctor_row = vm.rows.iter().find(|r| r.id == "doctor").unwrap();
        assert_eq!(doctor_row.status, InitChecklistStatus::Ready);
        assert_eq!(doctor_row.action, InitChecklistAction::None);
    }

    #[test]
    fn next_action_missing_model_first() {
        let inputs = InitChecklistInputs {
            active_model_label: None,
            approvals_count: 5,
            doctor_status: DoctorCheckStatus::Error,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        assert_eq!(
            vm.next_action.as_deref(),
            Some("select a model with /model")
        );
    }

    #[test]
    fn next_action_pending_approvals_second() {
        let inputs = InitChecklistInputs {
            approvals_count: 3,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        assert_eq!(vm.next_action.as_deref(), Some("review pending approvals"));
    }

    #[test]
    fn next_action_doctor_error_third() {
        let inputs = InitChecklistInputs {
            doctor_status: DoctorCheckStatus::Error,
            ..all_ready_inputs()
        };
        let vm = project_init_checklist(inputs);
        assert_eq!(
            vm.next_action.as_deref(),
            Some("run /doctor to fix critical issues")
        );
    }

    #[test]
    fn next_action_ready_when_all_clear() {
        let vm = project_init_checklist(all_ready_inputs());
        assert_eq!(vm.next_action.as_deref(), Some("ready to start"));
    }

    #[test]
    fn produces_five_rows() {
        let vm = project_init_checklist(all_ready_inputs());
        assert_eq!(vm.rows.len(), 5);
    }
}
