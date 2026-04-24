//! U8 — `vac onboard` — post-install suggested first-steps checklist.
//!
//! Minimal-viable onboarding that rides on the unified grammar from
//! U0-U7. Rather than a full interactive wizard (which needs richer
//! TUI interactivity than a thin CLI command provides), we print a
//! prioritised checklist based on what's configured vs missing in
//! the project's `.vac/` dir. Each line is self-descriptive:
//!
//!     [✓]  config.toml detected
//!     [✗]  policy.toml missing       → vac sandbox-toggle
//!     [·]  no LSP server on PATH     → install rust-analyzer or set VAC_LSP_RUST_SERVER
//!
//! Runs headless, no network, no LLM. Exits 0 always — the output is
//! informational, not a pass/fail.

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Severity for one checklist row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardStatus {
    Ok,
    Info,
    Missing,
}

impl OnboardStatus {
    fn glyph(self) -> char {
        match self {
            Self::Ok => '✓',
            Self::Info => '·',
            Self::Missing => '✗',
        }
    }
}

#[derive(Debug, Clone)]
pub struct OnboardItem {
    pub status: OnboardStatus,
    pub label: String,
    pub suggestion: Option<String>,
}

pub async fn execute(project_root: PathBuf) -> anyhow::Result<()> {
    println!("── vac onboard — suggested first-steps checklist ──");
    let items = scan(&project_root).await;
    for item in &items {
        let suggestion = match &item.suggestion {
            Some(s) => format!("  → {s}"),
            None => String::new(),
        };
        println!(
            "[{}]  {}{}",
            item.status.glyph(),
            item.label,
            suggestion
        );
    }
    println!();
    let missing = items
        .iter()
        .filter(|i| i.status == OnboardStatus::Missing)
        .count();
    if missing > 0 {
        println!(
            "{missing} checklist item(s) suggest action. Fix or \
             acknowledge, then re-run `vac onboard` to confirm."
        );
    } else {
        println!("All first-steps satisfied. Happy coding.");
    }
    Ok(())
}

pub(crate) async fn scan(project_root: &Path) -> Vec<OnboardItem> {
    let mut out = Vec::new();
    out.push(check_config_toml(project_root).await);
    out.push(check_policy_toml(project_root).await);
    out.push(check_sandbox_toml(project_root).await);
    out.push(check_memory_dir(project_root).await);
    out.push(check_lsp_server("rust-analyzer").await);
    out.push(check_git(project_root).await);
    out
}

async fn check_config_toml(project_root: &Path) -> OnboardItem {
    let path = project_root.join(".vac/config.toml");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        OnboardItem {
            status: OnboardStatus::Ok,
            label: "config.toml detected".into(),
            suggestion: None,
        }
    } else {
        OnboardItem {
            status: OnboardStatus::Missing,
            label: ".vac/config.toml missing".into(),
            suggestion: Some("vac init".into()),
        }
    }
}

async fn check_policy_toml(project_root: &Path) -> OnboardItem {
    let path = project_root.join(".vac/policy.toml");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        OnboardItem {
            status: OnboardStatus::Ok,
            label: "policy.toml detected".into(),
            suggestion: None,
        }
    } else {
        OnboardItem {
            status: OnboardStatus::Info,
            label: "policy.toml absent (unlimited caps by default)".into(),
            suggestion: Some(
                "create .vac/policy.toml with max_submits_per_hour / \
                 max_tokens_per_session / denied_tools when you want \
                 org limits"
                    .into(),
            ),
        }
    }
}

async fn check_sandbox_toml(project_root: &Path) -> OnboardItem {
    let path = project_root.join(".vac/sandbox.toml");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        OnboardItem {
            status: OnboardStatus::Ok,
            label: "sandbox.toml detected".into(),
            suggestion: None,
        }
    } else {
        OnboardItem {
            status: OnboardStatus::Info,
            label: "sandbox.toml absent (running in host mode)".into(),
            suggestion: Some("vac sandbox-toggle to cycle environment_mode".into()),
        }
    }
}

async fn check_memory_dir(project_root: &Path) -> OnboardItem {
    let path = project_root.join(".vac/memory");
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        OnboardItem {
            status: OnboardStatus::Ok,
            label: ".vac/memory/ initialised".into(),
            suggestion: None,
        }
    } else {
        OnboardItem {
            status: OnboardStatus::Info,
            label: ".vac/memory/ absent (will be created on first use)".into(),
            suggestion: None,
        }
    }
}

async fn check_lsp_server(name: &str) -> OnboardItem {
    // `which`-style PATH probe via env. We avoid spawning the real
    // binary because some LSP servers take >1s to respond.
    let on_path = std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .find(|dir| dir.join(name).is_file())
        })
        .is_some();
    if on_path {
        OnboardItem {
            status: OnboardStatus::Ok,
            label: format!("{name} on PATH"),
            suggestion: None,
        }
    } else {
        OnboardItem {
            status: OnboardStatus::Info,
            label: format!("{name} not on PATH (LSP features degraded)"),
            suggestion: Some(format!(
                "install {name} or set VAC_LSP_RUST_SERVER to an alternative"
            )),
        }
    }
}

async fn check_git(project_root: &Path) -> OnboardItem {
    let result = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_root)
        .output()
        .await;
    match result {
        Ok(out) if out.status.success() => OnboardItem {
            status: OnboardStatus::Ok,
            label: "git repo detected".into(),
            suggestion: None,
        },
        _ => OnboardItem {
            status: OnboardStatus::Info,
            label: "not a git working tree (AwaySummary / bughunter degraded)".into(),
            suggestion: Some("git init if you want commit-aware features".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clean_tempdir_reports_everything_missing_or_info() {
        let tmp = tempfile::tempdir().unwrap();
        let items = scan(tmp.path()).await;
        assert!(!items.is_empty());
        // The config.toml check is the one "Missing" we expect
        // without setup.
        let missing: Vec<&OnboardItem> = items
            .iter()
            .filter(|i| i.status == OnboardStatus::Missing)
            .collect();
        assert!(missing.iter().any(|i| i.label.contains("config.toml")));
    }

    #[tokio::test]
    async fn config_present_reports_ok() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join(".vac")).await.unwrap();
        tokio::fs::write(tmp.path().join(".vac/config.toml"), "").await.unwrap();
        let item = check_config_toml(tmp.path()).await;
        assert_eq!(item.status, OnboardStatus::Ok);
    }

    #[tokio::test]
    async fn policy_missing_is_info_not_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let item = check_policy_toml(tmp.path()).await;
        // Policy absence is explicitly INFO — unlimited caps are a
        // valid default; "Missing" would scream at fresh projects.
        assert_eq!(item.status, OnboardStatus::Info);
    }

    #[tokio::test]
    async fn lsp_probe_for_bogus_binary_returns_info() {
        let item = check_lsp_server("definitely-not-a-real-binary-xyz").await;
        assert_eq!(item.status, OnboardStatus::Info);
        assert!(item.suggestion.is_some());
    }

    #[test]
    fn glyph_mapping_stable() {
        assert_eq!(OnboardStatus::Ok.glyph(), '✓');
        assert_eq!(OnboardStatus::Info.glyph(), '·');
        assert_eq!(OnboardStatus::Missing.glyph(), '✗');
    }

    #[tokio::test]
    async fn execute_runs_to_completion_on_empty_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        execute(tmp.path().to_path_buf()).await.unwrap();
    }
}
