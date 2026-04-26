use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use vac_shell_contracts::VacPaths;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoctorCheckStatus {
    Ok,
    Warning,
    Error,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub id: String,
    pub label: String,
    pub status: DoctorCheckStatus,
    pub summary: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub overall_status: DoctorCheckStatus,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDispatcherMode {
    Inert,
    Live,
}

#[derive(Debug, Clone)]
pub struct DoctorConfig {
    pub tool_dispatcher_mode: ToolDispatcherMode,
    pub check_boundary_gates: bool,
}

impl Default for DoctorConfig {
    fn default() -> Self {
        Self {
            tool_dispatcher_mode: ToolDispatcherMode::Inert,
            check_boundary_gates: false,
        }
    }
}

pub fn run_doctor_checks(paths: &dyn VacPaths, config: &DoctorConfig) -> DoctorReport {
    let mut checks = Vec::new();

    // 1. Paths check
    let mut paths_status = DoctorCheckStatus::Ok;
    let mut paths_details = Vec::new();

    let pr_exists = paths.project_root().exists();
    if !pr_exists {
        paths_status = DoctorCheckStatus::Error;
        paths_details.push("Project root does not exist".to_string());
    }

    let ps_exists = paths.project_state_dir().exists();
    if !ps_exists {
        if paths_status == DoctorCheckStatus::Ok {
            paths_status = DoctorCheckStatus::Warning;
        }
        paths_details.push("Project state dir does not exist".to_string());
    }

    let s_exists = paths.sessions_dir().exists();
    if !s_exists {
        if paths_status == DoctorCheckStatus::Ok {
            paths_status = DoctorCheckStatus::Warning;
        }
        paths_details.push("Sessions dir does not exist".to_string());
    }

    let m_parent_exists = paths
        .model_config_file()
        .parent()
        .map(|p| p.exists())
        .unwrap_or(false);
    if !m_parent_exists {
        if paths_status == DoctorCheckStatus::Ok {
            paths_status = DoctorCheckStatus::Warning;
        }
        paths_details.push("Model config parent dir does not exist".to_string());
    }

    checks.push(DoctorCheck {
        id: "paths".to_string(),
        label: ".vac paths metadata".to_string(),
        status: paths_status,
        summary: if paths_details.is_empty() {
            "Paths are available".to_string()
        } else {
            "Path issues detected".to_string()
        },
        detail: if paths_details.is_empty() {
            None
        } else {
            Some(paths_details.join(", "))
        },
    });

    // 2 & 3. Model Config & Credentials
    let mut mc_status = DoctorCheckStatus::Ok;
    let mut mc_summary = "Model config ok".to_string();
    let mut mc_detail = None;

    let mut cred_status = DoctorCheckStatus::Ok;
    let mut cred_summary = "Credentials present".to_string();
    let mut cred_detail = None;

    if !paths.model_config_file().exists() {
        mc_status = DoctorCheckStatus::Warning;
        mc_summary = "Model config snapshot missing".to_string();
        cred_status = DoctorCheckStatus::Skipped;
        cred_summary = "Cannot check credentials without model config".to_string();
    } else {
        match vac_shell_host_vac_config::load_from_paths(paths) {
            Ok(Some(snap)) => {
                use vac_shell_host_vac_config::VacModelConfigSnapshot;
                let active = snap.active();
                let sanitized = vac_shell_host_vac_config::sanitize_active_model(
                    &snap.providers(),
                    &snap.models(),
                    active.clone(),
                );

                if active.is_some() && sanitized.is_none() {
                    mc_status = DoctorCheckStatus::Error;
                    mc_summary = "Active model is invalid or missing credentials".to_string();
                } else if sanitized.is_none() {
                    mc_status = DoctorCheckStatus::Warning;
                    mc_summary = "No active model configured".to_string();
                }

                // Check credentials
                let mut cred_issues = Vec::new();
                for p in snap.providers() {
                    if !p.credentials_present {
                        let env_name = match p.id.0.as_str() {
                            "anthropic" => "ANTHROPIC_API_KEY",
                            "openai" => "OPENAI_API_KEY",
                            "gemini" => "GEMINI_API_KEY",
                            _ => "",
                        };
                        let env_present = if !env_name.is_empty() {
                            std::env::var_os(env_name).is_some()
                        } else {
                            false
                        };
                        if !env_present {
                            cred_status = DoctorCheckStatus::Warning;
                            cred_issues
                                .push(format!("Provider {} missing credentials", p.id.0.as_str()));
                        }
                    }
                }
                if !cred_issues.is_empty() {
                    cred_status = DoctorCheckStatus::Warning;
                    cred_summary = "Missing provider credentials".to_string();
                    cred_detail = Some(cred_issues.join(", "));
                }
            }
            Ok(None) => {
                mc_status = DoctorCheckStatus::Warning;
                mc_summary = "Model config snapshot is empty".to_string();
                cred_status = DoctorCheckStatus::Skipped;
                cred_summary = "Cannot check credentials without model config".to_string();
            }
            Err(e) => {
                mc_status = DoctorCheckStatus::Error;
                mc_summary = "Model config parse error".to_string();
                mc_detail = Some(e.to_string());
                cred_status = DoctorCheckStatus::Skipped;
                cred_summary = "Cannot check credentials on corrupt config".to_string();
            }
        }
    }

    checks.push(DoctorCheck {
        id: "model_config".to_string(),
        label: "Model config snapshot".to_string(),
        status: mc_status,
        summary: mc_summary,
        detail: mc_detail,
    });

    checks.push(DoctorCheck {
        id: "credentials".to_string(),
        label: "Provider credentials".to_string(),
        status: cred_status,
        summary: cred_summary,
        detail: cred_detail,
    });

    // 4. Tool dispatcher mode
    let td_mode_str = match config.tool_dispatcher_mode {
        ToolDispatcherMode::Inert => "inert",
        ToolDispatcherMode::Live => "live",
    };
    checks.push(DoctorCheck {
        id: "dispatcher_mode".to_string(),
        label: "Tool dispatcher mode".to_string(),
        status: DoctorCheckStatus::Ok,
        summary: format!("Mode: {}", td_mode_str),
        detail: None,
    });

    // 5. Boundary gates
    if config.check_boundary_gates {
        let script_path = paths.project_root().join("scripts/check-dtrack-gates.sh");
        let (bg_status, bg_summary) = if script_path.exists() {
            let meta = std::fs::metadata(&script_path);
            if let Ok(m) = meta {
                if m.permissions().mode() & 0o111 != 0 {
                    (
                        DoctorCheckStatus::Ok,
                        "Boundary gate script available and executable".to_string(),
                    )
                } else {
                    (
                        DoctorCheckStatus::Warning,
                        "Boundary gate script is not executable".to_string(),
                    )
                }
            } else {
                (
                    DoctorCheckStatus::Warning,
                    "Boundary gate script metadata inaccessible".to_string(),
                )
            }
        } else {
            (
                DoctorCheckStatus::Warning,
                "Boundary gate script missing".to_string(),
            )
        };

        checks.push(DoctorCheck {
            id: "boundary_gates".to_string(),
            label: "Boundary gates".to_string(),
            status: bg_status,
            summary: bg_summary,
            detail: None,
        });
    }

    let overall = if checks.iter().any(|c| c.status == DoctorCheckStatus::Error) {
        DoctorCheckStatus::Error
    } else if checks
        .iter()
        .any(|c| c.status == DoctorCheckStatus::Warning)
    {
        DoctorCheckStatus::Warning
    } else {
        DoctorCheckStatus::Ok
    };

    DoctorReport {
        overall_status: overall,
        checks,
    }
}
